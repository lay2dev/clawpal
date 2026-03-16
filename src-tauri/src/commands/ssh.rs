use super::*;

pub type SshConfigHostSuggestion = clawpal_core::ssh::config::SshConfigHostSuggestion;

fn ssh_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("config"))
}

pub(crate) fn read_hosts_from_registry() -> Result<Vec<SshHostConfig>, String> {
    clawpal_core::ssh::registry::list_ssh_hosts()
}

#[tauri::command]
pub fn list_ssh_hosts() -> Result<Vec<SshHostConfig>, String> {
    read_hosts_from_registry()
}

#[tauri::command]
pub fn list_ssh_config_hosts() -> Result<Vec<SshConfigHostSuggestion>, String> {
    let Some(path) = ssh_config_path() else {
        return Ok(Vec::new());
    };
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    Ok(clawpal_core::ssh::config::parse_ssh_config_hosts(&data))
}

#[tauri::command]
pub fn upsert_ssh_host(host: SshHostConfig) -> Result<SshHostConfig, String> {
    clawpal_core::ssh::registry::upsert_ssh_host(host)
}

#[tauri::command]
pub fn delete_ssh_host(host_id: String) -> Result<bool, String> {
    clawpal_core::ssh::registry::delete_ssh_host(&host_id)
}

// ---------------------------------------------------------------------------
// SSH connect / disconnect / status
// ---------------------------------------------------------------------------

fn emit_ssh_diagnostic(app: &AppHandle, report: &SshDiagnosticReport) {
    let code = report.error_code.map(|value| value.as_str().to_string());
    let payload = json!({
        "stage": report.stage,
        "intent": report.intent,
        "status": report.status,
        "errorCode": code,
        "summary": report.summary,
        "repairPlan": report.repair_plan,
        "confidence": report.confidence,
    });
    let _ = app.emit("ssh:diagnostic", payload.clone());
    if !report.repair_plan.is_empty() {
        let _ = app.emit("ssh:repair-suggested", payload.clone());
    }
    crate::logging::log_info(&format!("[ssh:diagnostic] {payload}"));
}

fn make_ssh_command_error(
    app: &AppHandle,
    stage: SshStage,
    intent: SshIntent,
    raw: impl Into<String>,
) -> String {
    let message = raw.into();
    let diagnostic = from_any_error(stage, intent, message.clone());
    emit_ssh_diagnostic(app, &diagnostic);
    message
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SshDiagnosticSuccessTrigger {
    ConnectEstablished,
    ConnectReuse,
    ExplicitProbe,
    RoutineOperation,
}

fn should_emit_success_ssh_diagnostic(trigger: SshDiagnosticSuccessTrigger) -> bool {
    matches!(
        trigger,
        SshDiagnosticSuccessTrigger::ConnectEstablished
            | SshDiagnosticSuccessTrigger::ExplicitProbe
    )
}

fn success_ssh_diagnostic(
    app: &AppHandle,
    stage: SshStage,
    intent: SshIntent,
    summary: impl Into<String>,
    trigger: SshDiagnosticSuccessTrigger,
) -> SshDiagnosticReport {
    let report = SshDiagnosticReport::success(stage, intent, summary);
    if should_emit_success_ssh_diagnostic(trigger) {
        emit_ssh_diagnostic(app, &report);
    }
    report
}

fn skipped_probe_diagnostic(
    stage: SshStage,
    intent: SshIntent,
    summary: impl Into<String>,
) -> SshDiagnosticReport {
    SshDiagnosticReport {
        stage,
        intent,
        status: SshDiagnosticStatus::Degraded,
        error_code: None,
        summary: summary.into(),
        evidence: Vec::new(),
        repair_plan: Vec::new(),
        confidence: 0.5,
    }
}

fn ssh_stage_for_error_code(code: SshErrorCode) -> SshStage {
    match code {
        SshErrorCode::HostUnreachable | SshErrorCode::ConnectionRefused | SshErrorCode::Timeout => {
            SshStage::TcpReachability
        }
        SshErrorCode::HostKeyFailed => SshStage::HostKeyVerification,
        SshErrorCode::KeyfileMissing
        | SshErrorCode::PassphraseRequired
        | SshErrorCode::AuthFailed
        | SshErrorCode::SftpPermissionDenied => SshStage::AuthNegotiation,
        SshErrorCode::SessionStale => SshStage::SessionOpen,
        SshErrorCode::RemoteCommandFailed => SshStage::RemoteExec,
        SshErrorCode::Unknown => SshStage::TcpReachability,
    }
}

fn ssh_stage_for_intent(intent: SshIntent) -> SshStage {
    match intent {
        SshIntent::Connect => SshStage::SessionOpen,
        SshIntent::Exec
        | SshIntent::InstallStep
        | SshIntent::DoctorRemote
        | SshIntent::HealthCheck => SshStage::RemoteExec,
        SshIntent::SftpRead => SshStage::SftpRead,
        SshIntent::SftpWrite => SshStage::SftpWrite,
        SshIntent::SftpRemove => SshStage::SftpRemove,
    }
}

#[cfg(test)]
mod ssh_diagnostic_policy_tests {
    use super::{
        should_emit_success_ssh_diagnostic, skipped_probe_diagnostic, SshDiagnosticSuccessTrigger,
    };
    use clawpal_core::ssh::diagnostic::{SshDiagnosticStatus, SshIntent, SshStage};

    #[test]
    fn suppresses_routine_success_diagnostics() {
        assert!(!should_emit_success_ssh_diagnostic(
            SshDiagnosticSuccessTrigger::RoutineOperation
        ));
        assert!(!should_emit_success_ssh_diagnostic(
            SshDiagnosticSuccessTrigger::ConnectReuse
        ));
    }

    #[test]
    fn keeps_meaningful_success_diagnostics() {
        assert!(should_emit_success_ssh_diagnostic(
            SshDiagnosticSuccessTrigger::ConnectEstablished
        ));
        assert!(should_emit_success_ssh_diagnostic(
            SshDiagnosticSuccessTrigger::ExplicitProbe
        ));
    }

    #[test]
    fn skipped_probes_report_degraded_status() {
        let report = skipped_probe_diagnostic(
            SshStage::SftpWrite,
            SshIntent::SftpWrite,
            "SFTP write probe skipped (no-op)",
        );

        assert_eq!(report.status, SshDiagnosticStatus::Degraded);
        assert_eq!(report.error_code, None);
    }
}

#[tauri::command]
pub async fn ssh_connect(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    app: AppHandle,
) -> Result<bool, String> {
    crate::commands::logs::log_dev(format!("[dev][ssh_connect] begin host_id={host_id}"));
    // If already connected and handle is alive, reuse
    if pool.is_connected(&host_id).await {
        crate::commands::logs::log_dev(format!(
            "[dev][ssh_connect] reuse existing connection host_id={host_id}"
        ));
        let _ = success_ssh_diagnostic(
            &app,
            SshStage::SessionOpen,
            SshIntent::Connect,
            "SSH session already connected",
            SshDiagnosticSuccessTrigger::ConnectReuse,
        );
        return Ok(true);
    }
    let hosts = read_hosts_from_registry().map_err(|error| {
        make_ssh_command_error(&app, SshStage::ResolveHostConfig, SshIntent::Connect, error)
    })?;
    if hosts.is_empty() {
        crate::commands::logs::log_dev("[dev][ssh_connect] host registry is empty");
    }
    let host = hosts.into_iter().find(|h| h.id == host_id).ok_or_else(|| {
        let mut ids = Vec::new();
        for h in read_hosts_from_registry().unwrap_or_default() {
            ids.push(h.id);
        }
        crate::commands::logs::log_dev(format!(
            "[dev][ssh_connect] no host found host_id={host_id} known={ids:?}"
        ));
        make_ssh_command_error(
            &app,
            SshStage::ResolveHostConfig,
            SshIntent::Connect,
            format!("No SSH host config with id: {host_id}"),
        )
    })?;
    // If the host has a stored passphrase, use it directly
    let connect_result = if let Some(ref pp) = host.passphrase {
        if !pp.is_empty() {
            crate::commands::logs::log_dev(format!(
                "[dev][ssh_connect] using stored passphrase for host_id={host_id}"
            ));
            pool.connect_with_passphrase(&host, Some(pp.as_str())).await
        } else {
            pool.connect(&host).await
        }
    } else {
        pool.connect(&host).await
    };
    if let Err(error) = connect_result {
        crate::commands::logs::log_dev(format!(
            "[dev][ssh_connect] failed host_id={} host={} user={} port={} auth_method={} error={}",
            host_id, host.host, host.username, host.port, host.auth_method, error
        ));
        let message = format!("ssh connect failed: {error}");
        let mut diagnostic = from_any_error(
            SshStage::TcpReachability,
            SshIntent::Connect,
            message.clone(),
        );
        if let Some(code) = diagnostic.error_code {
            diagnostic.stage = ssh_stage_for_error_code(code);
        }
        emit_ssh_diagnostic(&app, &diagnostic);
        return Err(message);
    }
    crate::commands::logs::log_dev(format!("[dev][ssh_connect] success host_id={host_id}"));
    let _ = success_ssh_diagnostic(
        &app,
        SshStage::SessionOpen,
        SshIntent::Connect,
        "SSH connection established",
        SshDiagnosticSuccessTrigger::ConnectEstablished,
    );
    Ok(true)
}

#[tauri::command]
pub async fn ssh_connect_with_passphrase(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    passphrase: String,
    app: AppHandle,
) -> Result<bool, String> {
    crate::commands::logs::log_dev(format!(
        "[dev][ssh_connect_with_passphrase] begin host_id={host_id}"
    ));
    if pool.is_connected(&host_id).await {
        crate::commands::logs::log_dev(format!(
            "[dev][ssh_connect_with_passphrase] reuse existing connection host_id={host_id}"
        ));
        let _ = success_ssh_diagnostic(
            &app,
            SshStage::SessionOpen,
            SshIntent::Connect,
            "SSH session already connected",
            SshDiagnosticSuccessTrigger::ConnectReuse,
        );
        return Ok(true);
    }
    let hosts = read_hosts_from_registry().map_err(|error| {
        make_ssh_command_error(&app, SshStage::ResolveHostConfig, SshIntent::Connect, error)
    })?;
    if hosts.is_empty() {
        crate::commands::logs::log_dev("[dev][ssh_connect_with_passphrase] host registry is empty");
    }
    let host = hosts.into_iter().find(|h| h.id == host_id).ok_or_else(|| {
        let mut ids = Vec::new();
        for h in read_hosts_from_registry().unwrap_or_default() {
            ids.push(h.id);
        }
        crate::commands::logs::log_dev(format!(
            "[dev][ssh_connect_with_passphrase] no host found host_id={host_id} known={ids:?}"
        ));
        make_ssh_command_error(
            &app,
            SshStage::ResolveHostConfig,
            SshIntent::Connect,
            format!("No SSH host config with id: {host_id}"),
        )
    })?;
    if let Err(error) = pool
        .connect_with_passphrase(&host, Some(passphrase.as_str()))
        .await
    {
        crate::commands::logs::log_dev(format!(
            "[dev][ssh_connect_with_passphrase] failed host_id={} host={} user={} port={} auth_method={} error={}",
            host_id,
            host.host,
            host.username,
            host.port,
            host.auth_method,
            error
        ));
        return Err(make_ssh_command_error(
            &app,
            SshStage::AuthNegotiation,
            SshIntent::Connect,
            format!("ssh connect failed: {error}"),
        ));
    }
    crate::commands::logs::log_dev(format!(
        "[dev][ssh_connect_with_passphrase] success host_id={host_id}"
    ));
    let _ = success_ssh_diagnostic(
        &app,
        SshStage::SessionOpen,
        SshIntent::Connect,
        "SSH connection established",
        SshDiagnosticSuccessTrigger::ConnectEstablished,
    );
    Ok(true)
}

#[tauri::command]
pub async fn ssh_disconnect(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<bool, String> {
    pool.disconnect(&host_id).await?;
    Ok(true)
}

#[tauri::command]
pub async fn ssh_status(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<String, String> {
    if pool.is_connected(&host_id).await {
        Ok("connected".to_string())
    } else {
        Ok("disconnected".to_string())
    }
}

#[tauri::command]
pub async fn get_ssh_transfer_stats(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<SshTransferStats, String> {
    Ok(pool.get_transfer_stats(&host_id).await)
}

// ---------------------------------------------------------------------------
// SSH exec and SFTP Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn ssh_exec(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    command: String,
    app: AppHandle,
) -> Result<SshExecResult, String> {
    pool.exec(&host_id, &command)
        .await
        .map(|result| {
            let _ = success_ssh_diagnostic(
                &app,
                SshStage::RemoteExec,
                SshIntent::Exec,
                "Remote SSH command executed",
                SshDiagnosticSuccessTrigger::RoutineOperation,
            );
            result
        })
        .map_err(|error| make_ssh_command_error(&app, SshStage::RemoteExec, SshIntent::Exec, error))
}

#[tauri::command]
pub async fn sftp_read_file(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    path: String,
    app: AppHandle,
) -> Result<String, String> {
    pool.sftp_read(&host_id, &path)
        .await
        .map(|result| {
            let _ = success_ssh_diagnostic(
                &app,
                SshStage::SftpRead,
                SshIntent::SftpRead,
                "SFTP read succeeded",
                SshDiagnosticSuccessTrigger::RoutineOperation,
            );
            result
        })
        .map_err(|error| {
            make_ssh_command_error(&app, SshStage::SftpRead, SshIntent::SftpRead, error)
        })
}

#[tauri::command]
pub async fn sftp_write_file(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    path: String,
    content: String,
    app: AppHandle,
) -> Result<bool, String> {
    pool.sftp_write(&host_id, &path, &content)
        .await
        .map_err(|error| {
            make_ssh_command_error(&app, SshStage::SftpWrite, SshIntent::SftpWrite, error)
        })?;
    let _ = success_ssh_diagnostic(
        &app,
        SshStage::SftpWrite,
        SshIntent::SftpWrite,
        "SFTP write succeeded",
        SshDiagnosticSuccessTrigger::RoutineOperation,
    );
    Ok(true)
}

#[tauri::command]
pub async fn sftp_list_dir(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    path: String,
    app: AppHandle,
) -> Result<Vec<SftpEntry>, String> {
    pool.sftp_list(&host_id, &path)
        .await
        .map(|result| {
            let _ = success_ssh_diagnostic(
                &app,
                SshStage::SftpRead,
                SshIntent::SftpRead,
                "SFTP list succeeded",
                SshDiagnosticSuccessTrigger::RoutineOperation,
            );
            result
        })
        .map_err(|error| {
            make_ssh_command_error(&app, SshStage::SftpRead, SshIntent::SftpRead, error)
        })
}

#[tauri::command]
pub async fn sftp_remove_file(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    path: String,
    app: AppHandle,
) -> Result<bool, String> {
    pool.sftp_remove(&host_id, &path).await.map_err(|error| {
        make_ssh_command_error(&app, SshStage::SftpRemove, SshIntent::SftpRemove, error)
    })?;
    let _ = success_ssh_diagnostic(
        &app,
        SshStage::SftpRemove,
        SshIntent::SftpRemove,
        "SFTP remove succeeded",
        SshDiagnosticSuccessTrigger::RoutineOperation,
    );
    Ok(true)
}

#[tauri::command]
pub async fn diagnose_ssh(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    intent: String,
    app: AppHandle,
) -> Result<SshDiagnosticReport, String> {
    let intent = intent.parse::<SshIntent>().map_err(|_| {
        make_ssh_command_error(
            &app,
            SshStage::ResolveHostConfig,
            SshIntent::Connect,
            format!("Invalid SSH diagnostic intent: {intent}"),
        )
    })?;

    let stage = ssh_stage_for_intent(intent);
    if matches!(intent, SshIntent::Connect) {
        if pool.is_connected(&host_id).await {
            return Ok(success_ssh_diagnostic(
                &app,
                stage,
                intent,
                "SSH connection is healthy",
                SshDiagnosticSuccessTrigger::ExplicitProbe,
            ));
        }
        let hosts = read_hosts_from_registry().map_err(|error| {
            make_ssh_command_error(&app, SshStage::ResolveHostConfig, SshIntent::Connect, error)
        })?;
        let host = hosts.into_iter().find(|h| h.id == host_id).ok_or_else(|| {
            make_ssh_command_error(
                &app,
                SshStage::ResolveHostConfig,
                SshIntent::Connect,
                format!("No SSH host config with id: {host_id}"),
            )
        })?;
        return Ok(match pool.connect(&host).await {
            Ok(_) => success_ssh_diagnostic(
                &app,
                SshStage::SessionOpen,
                SshIntent::Connect,
                "SSH connect probe succeeded",
                SshDiagnosticSuccessTrigger::ExplicitProbe,
            ),
            Err(error) => {
                let mut report =
                    from_any_error(SshStage::TcpReachability, SshIntent::Connect, error);
                if let Some(code) = report.error_code {
                    report.stage = ssh_stage_for_error_code(code);
                }
                emit_ssh_diagnostic(&app, &report);
                report
            }
        });
    }

    if !pool.is_connected(&host_id).await {
        let report = from_any_error(stage, intent, format!("No connection for id: {host_id}"));
        emit_ssh_diagnostic(&app, &report);
        return Ok(report);
    }

    let report = match intent {
        SshIntent::Exec
        | SshIntent::InstallStep
        | SshIntent::DoctorRemote
        | SshIntent::HealthCheck => {
            match pool.exec(&host_id, "echo clawpal_ssh_diagnostic").await {
                Ok(_) => SshDiagnosticReport::success(stage, intent, "SSH exec probe succeeded"),
                Err(error) => from_any_error(stage, intent, error),
            }
        }
        SshIntent::SftpRead => match pool.sftp_list(&host_id, "~").await {
            Ok(_) => SshDiagnosticReport::success(stage, intent, "SFTP read probe succeeded"),
            Err(error) => from_any_error(stage, intent, error),
        },
        SshIntent::SftpWrite => {
            skipped_probe_diagnostic(stage, intent, "SFTP write probe skipped (no-op)")
        }
        SshIntent::SftpRemove => {
            skipped_probe_diagnostic(stage, intent, "SFTP remove probe skipped (no-op)")
        }
        SshIntent::Connect => unreachable!(),
    };
    emit_ssh_diagnostic(&app, &report);
    Ok(report)
}
