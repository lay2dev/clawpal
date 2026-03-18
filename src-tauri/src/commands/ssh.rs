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
    timed_sync!("list_ssh_hosts", { read_hosts_from_registry() })
}

#[tauri::command]
pub fn list_ssh_config_hosts() -> Result<Vec<SshConfigHostSuggestion>, String> {
    timed_sync!("list_ssh_config_hosts", {
        let Some(path) = ssh_config_path() else {
            return Ok(Vec::new());
        };
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        Ok(clawpal_core::ssh::config::parse_ssh_config_hosts(&data))
    })
}

#[tauri::command]
pub fn upsert_ssh_host(host: SshHostConfig) -> Result<SshHostConfig, String> {
    timed_sync!("upsert_ssh_host", {
        clawpal_core::ssh::registry::upsert_ssh_host(host)
    })
}

#[tauri::command]
pub fn delete_ssh_host(host_id: String) -> Result<bool, String> {
    timed_sync!("delete_ssh_host", {
        clawpal_core::ssh::registry::delete_ssh_host(&host_id)
    })
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
    timed_async!("ssh_connect", {
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
    })
}

#[tauri::command]
pub async fn ssh_connect_with_passphrase(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    passphrase: String,
    app: AppHandle,
) -> Result<bool, String> {
    timed_async!("ssh_connect_with_passphrase", {
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
            crate::commands::logs::log_dev(
                "[dev][ssh_connect_with_passphrase] host registry is empty",
            );
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
    })
}

#[tauri::command]
pub async fn ssh_disconnect(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<bool, String> {
    timed_async!("ssh_disconnect", {
        pool.disconnect(&host_id).await?;
        Ok(true)
    })
}

#[tauri::command]
pub async fn ssh_status(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<String, String> {
    timed_async!("ssh_status", {
        if pool.is_connected(&host_id).await {
            Ok("connected".to_string())
        } else {
            Ok("disconnected".to_string())
        }
    })
}

#[tauri::command]
pub async fn get_ssh_transfer_stats(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<SshTransferStats, String> {
    timed_async!("get_ssh_transfer_stats", {
        Ok(pool.get_transfer_stats(&host_id).await)
    })
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
    timed_async!("ssh_exec", {
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
            .map_err(|error| {
                make_ssh_command_error(&app, SshStage::RemoteExec, SshIntent::Exec, error)
            })
    })
}

#[tauri::command]
pub async fn sftp_read_file(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    path: String,
    app: AppHandle,
) -> Result<String, String> {
    timed_async!("sftp_read_file", {
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
    timed_async!("sftp_write_file", {
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
    })
}

#[tauri::command]
pub async fn sftp_list_dir(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    path: String,
    app: AppHandle,
) -> Result<Vec<SftpEntry>, String> {
    timed_async!("sftp_list_dir", {
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
    })
}

#[tauri::command]
pub async fn sftp_remove_file(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    path: String,
    app: AppHandle,
) -> Result<bool, String> {
    timed_async!("sftp_remove_file", {
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
    })
}

#[tauri::command]
pub async fn diagnose_ssh(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    intent: String,
    app: AppHandle,
) -> Result<SshDiagnosticReport, String> {
    timed_async!("diagnose_ssh", {
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
                    Ok(_) => {
                        SshDiagnosticReport::success(stage, intent, "SSH exec probe succeeded")
                    }
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
    })
}

// --- Extracted from mod.rs ---

pub(crate) fn is_owner_display_parse_error(text: &str) -> bool {
    clawpal_core::doctor::owner_display_parse_error(text)
}

pub(crate) async fn run_openclaw_remote_with_autofix(
    pool: &SshConnectionPool,
    host_id: &str,
    args: &[&str],
) -> Result<crate::cli_runner::CliOutput, String> {
    let first = crate::cli_runner::run_openclaw_remote(pool, host_id, args).await?;
    if first.exit_code == 0 {
        return Ok(first);
    }
    let combined = format!("{}\n{}", first.stderr, first.stdout);
    if !is_owner_display_parse_error(&combined) {
        return Ok(first);
    }
    let _ = crate::cli_runner::run_openclaw_remote(pool, host_id, &["doctor", "--fix"]).await;
    crate::cli_runner::run_openclaw_remote(pool, host_id, args).await
}

/// Private helper: snapshot current config then write new config on remote.
pub(crate) async fn remote_write_config_with_snapshot(
    pool: &SshConnectionPool,
    host_id: &str,
    config_path: &str,
    current_text: &str,
    next: &Value,
    source: &str,
) -> Result<(), String> {
    // Use core function to prepare config write
    let (new_text, snapshot_text) =
        clawpal_core::config::prepare_config_write(current_text, next, source)?;

    // Create snapshot dir
    pool.exec(host_id, "mkdir -p ~/.clawpal/snapshots").await?;

    // Generate snapshot filename
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let snapshot_path = clawpal_core::config::snapshot_filename(ts, source);
    let snapshot_full_path = format!("~/.clawpal/snapshots/{snapshot_path}");

    // Write snapshot and new config via SFTP
    pool.sftp_write(host_id, &snapshot_full_path, &snapshot_text)
        .await?;
    pool.sftp_write(host_id, config_path, &new_text).await?;
    Ok(())
}

pub(crate) async fn remote_resolve_openclaw_config_path(
    pool: &SshConnectionPool,
    host_id: &str,
) -> Result<String, String> {
    if let Ok(cache) = REMOTE_OPENCLAW_CONFIG_PATH_CACHE.lock() {
        if let Some((path, cached_at)) = cache.get(host_id) {
            if cached_at.elapsed() < REMOTE_OPENCLAW_CONFIG_PATH_CACHE_TTL {
                return Ok(path.clone());
            }
        }
    }
    let result = pool
        .exec_login(
            host_id,
            clawpal_core::doctor::remote_openclaw_config_path_probe_script(),
        )
        .await?;
    if result.exit_code != 0 {
        let details = format!("{}\n{}", result.stderr.trim(), result.stdout.trim());
        return Err(format!(
            "Failed to resolve remote openclaw config path ({}): {}",
            result.exit_code,
            details.trim()
        ));
    }
    let path = result.stdout.trim();
    if path.is_empty() {
        return Err("Remote openclaw config path probe returned empty output".into());
    }
    if let Ok(mut cache) = REMOTE_OPENCLAW_CONFIG_PATH_CACHE.lock() {
        cache.insert(host_id.to_string(), (path.to_string(), Instant::now()));
    }
    Ok(path.to_string())
}

pub(crate) async fn remote_read_openclaw_config_text_and_json(
    pool: &SshConnectionPool,
    host_id: &str,
) -> Result<(String, String, Value), String> {
    let config_path = remote_resolve_openclaw_config_path(pool, host_id).await?;
    let raw = pool.sftp_read(host_id, &config_path).await?;
    let (parsed, normalized) = clawpal_core::config::parse_and_normalize_config(&raw)
        .map_err(|e| format!("Failed to parse remote config: {e}"))?;
    Ok((config_path, normalized, parsed))
}

pub(crate) async fn run_remote_rescue_bot_command(
    pool: &SshConnectionPool,
    host_id: &str,
    command: Vec<String>,
) -> Result<RescueBotCommandResult, String> {
    let output = run_remote_openclaw_raw(pool, host_id, &command).await?;
    if is_gateway_status_command_output_incompatible(&output, &command) {
        let fallback_command = strip_gateway_status_json_flag(&command);
        if fallback_command != command {
            let fallback_output = run_remote_openclaw_raw(pool, host_id, &fallback_command).await?;
            return Ok(RescueBotCommandResult {
                command: fallback_command,
                output: fallback_output,
            });
        }
    }
    Ok(RescueBotCommandResult { command, output })
}

pub(crate) async fn run_remote_openclaw_raw(
    pool: &SshConnectionPool,
    host_id: &str,
    command: &[String],
) -> Result<OpenclawCommandOutput, String> {
    let args = command.iter().map(String::as_str).collect::<Vec<_>>();
    let raw = crate::cli_runner::run_openclaw_remote(pool, host_id, &args).await?;
    Ok(OpenclawCommandOutput {
        stdout: raw.stdout,
        stderr: raw.stderr,
        exit_code: raw.exit_code,
    })
}

pub(crate) async fn run_remote_openclaw_dynamic(
    pool: &SshConnectionPool,
    host_id: &str,
    command: Vec<String>,
) -> Result<OpenclawCommandOutput, String> {
    Ok(run_remote_rescue_bot_command(pool, host_id, command)
        .await?
        .output)
}

pub(crate) async fn run_remote_primary_doctor_with_fallback(
    pool: &SshConnectionPool,
    host_id: &str,
    profile: &str,
) -> Result<OpenclawCommandOutput, String> {
    let json_command = build_profile_command(profile, &["doctor", "--json", "--yes"]);
    let output = run_remote_openclaw_dynamic(pool, host_id, json_command).await?;
    if output.exit_code != 0
        && clawpal_core::doctor::doctor_json_option_unsupported(&output.stderr, &output.stdout)
    {
        let plain_command = build_profile_command(profile, &["doctor", "--yes"]);
        return run_remote_openclaw_dynamic(pool, host_id, plain_command).await;
    }
    Ok(output)
}

pub(crate) async fn run_remote_gateway_restart_fallback(
    pool: &SshConnectionPool,
    host_id: &str,
    profile: &str,
    commands: &mut Vec<RescueBotCommandResult>,
) -> Result<(), String> {
    let stop_command = vec![
        "--profile".to_string(),
        profile.to_string(),
        "gateway".to_string(),
        "stop".to_string(),
    ];
    let stop_result = run_remote_rescue_bot_command(pool, host_id, stop_command).await?;
    commands.push(stop_result);

    let start_command = vec![
        "--profile".to_string(),
        profile.to_string(),
        "gateway".to_string(),
        "start".to_string(),
    ];
    let start_result = run_remote_rescue_bot_command(pool, host_id, start_command).await?;
    if start_result.output.exit_code != 0 {
        return Err(command_failure_message(
            &start_result.command,
            &start_result.output,
        ));
    }
    commands.push(start_result);
    Ok(())
}

pub(crate) fn is_remote_missing_path_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("no such file")
        || lower.contains("no such file or directory")
        || lower.contains("not found")
        || lower.contains("cannot open")
}

pub(crate) async fn read_remote_env_var(
    pool: &SshConnectionPool,
    host_id: &str,
    name: &str,
) -> Result<Option<String>, String> {
    if !is_valid_env_var_name(name) {
        return Err(format!("Invalid environment variable name: {name}"));
    }

    let cmd = format!("printenv -- {name}");
    let out = pool
        .exec_login(host_id, &cmd)
        .await
        .map_err(|e| format!("Failed to read remote env var {name}: {e}"))?;

    if out.exit_code != 0 {
        return Ok(None);
    }

    let value = out.stdout.trim();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value.to_string()))
    }
}

pub(crate) async fn resolve_remote_key_from_agent_auth_profiles(
    pool: &SshConnectionPool,
    host_id: &str,
    auth_ref: &str,
) -> Result<Option<String>, String> {
    let roots = resolve_remote_openclaw_roots(pool, host_id).await?;

    for root in roots {
        let agents_path = format!("{}/agents", root.trim_end_matches('/'));
        let entries = match pool.sftp_list(host_id, &agents_path).await {
            Ok(entries) => entries,
            Err(e) if is_remote_missing_path_error(&e) => continue,
            Err(e) => {
                return Err(format!(
                    "Failed to list remote agents directory at {agents_path}: {e}"
                ))
            }
        };

        for agent in entries.into_iter().filter(|entry| entry.is_dir) {
            let agent_dir = format!("{}/agents/{}/agent", root.trim_end_matches('/'), agent.name);
            for file_name in ["auth-profiles.json", "auth.json"] {
                let auth_file = format!("{agent_dir}/{file_name}");
                let text = match pool.sftp_read(host_id, &auth_file).await {
                    Ok(text) => text,
                    Err(e) if is_remote_missing_path_error(&e) => continue,
                    Err(e) => {
                        return Err(format!(
                            "Failed to read remote auth store at {auth_file}: {e}"
                        ))
                    }
                };
                let data: Value = serde_json::from_str(&text).map_err(|e| {
                    format!("Failed to parse remote auth store at {auth_file}: {e}")
                })?;
                // Try plaintext first, then resolve SecretRef env vars from remote.
                if let Some(key) = resolve_key_from_auth_store_json(&data, auth_ref) {
                    return Ok(Some(key));
                }
                // Collect env-source SecretRef names and fetch them from remote host.
                let sr_env_names = collect_secret_ref_env_names_from_auth_store(&data);
                if !sr_env_names.is_empty() {
                    let remote_env =
                        RemoteAuthCache::batch_read_env_vars(pool, host_id, &sr_env_names)
                            .await
                            .unwrap_or_default();
                    let env_lookup =
                        |name: &str| -> Option<String> { remote_env.get(name).cloned() };
                    if let Some(key) =
                        resolve_key_from_auth_store_json_with_env(&data, auth_ref, &env_lookup)
                    {
                        return Ok(Some(key));
                    }
                }
            }
        }
    }

    Ok(None)
}

pub(crate) async fn resolve_remote_openclaw_roots(
    pool: &SshConnectionPool,
    host_id: &str,
) -> Result<Vec<String>, String> {
    let mut roots = Vec::<String>::new();
    let primary = pool
        .exec_login(
            host_id,
            clawpal_core::doctor::remote_openclaw_root_probe_script(),
        )
        .await?;
    let primary_trimmed = primary.stdout.trim();
    if !primary_trimmed.is_empty() {
        roots.push(primary_trimmed.to_string());
    }

    let discover = pool
        .exec_login(
            host_id,
            "for d in \"$HOME\"/.openclaw*; do [ -d \"$d\" ] && printf '%s\\n' \"$d\"; done",
        )
        .await?;
    for line in discover.stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            roots.push(trimmed.to_string());
        }
    }
    let mut deduped = Vec::<String>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();
    for root in roots {
        if seen.insert(root.clone()) {
            deduped.push(root);
        }
    }
    roots = deduped;
    Ok(roots)
}

pub(crate) async fn resolve_remote_profile_base_url(
    pool: &SshConnectionPool,
    host_id: &str,
    profile: &ModelProfile,
) -> Result<Option<String>, String> {
    if let Some(base) = profile
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return Ok(Some(base.to_string()));
    }

    let config_path = match remote_resolve_openclaw_config_path(pool, host_id).await {
        Ok(path) => path,
        Err(_) => return Ok(None),
    };
    let raw = match pool.sftp_read(host_id, &config_path).await {
        Ok(raw) => raw,
        Err(e) if is_remote_missing_path_error(&e) => return Ok(None),
        Err(e) => {
            return Err(format!(
                "Failed to read remote config for base URL resolution: {e}"
            ))
        }
    };
    let cfg = match clawpal_core::config::parse_and_normalize_config(&raw) {
        Ok((parsed, _)) => parsed,
        Err(e) => {
            return Err(format!(
                "Failed to parse remote config for base URL resolution: {e}"
            ))
        }
    };
    Ok(resolve_model_provider_base_url(&cfg, &profile.provider))
}

pub(crate) async fn resolve_remote_profile_api_key(
    pool: &SshConnectionPool,
    host_id: &str,
    profile: &ModelProfile,
) -> Result<String, String> {
    let auth_ref = profile.auth_ref.trim();
    let has_explicit_auth_ref = !auth_ref.is_empty();

    // 1. Explicit auth_ref (user-specified): env var, then auth store.
    if has_explicit_auth_ref {
        if is_valid_env_var_name(auth_ref) {
            if let Some(key) = read_remote_env_var(pool, host_id, auth_ref).await? {
                return Ok(key);
            }
        }
        if let Some(key) =
            resolve_remote_key_from_agent_auth_profiles(pool, host_id, auth_ref).await?
        {
            return Ok(key);
        }
    }

    // 2. Direct api_key before fallback auth refs/env conventions.
    if let Some(key) = &profile.api_key {
        let trimmed_key = key.trim();
        if !trimmed_key.is_empty() {
            return Ok(trimmed_key.to_string());
        }
    }

    // 3. Fallback provider:default auth_ref from auth store.
    let provider = profile.provider.trim().to_lowercase();
    if !provider.is_empty() {
        let fallback = format!("{provider}:default");
        let skip = has_explicit_auth_ref && auth_ref == fallback;
        if !skip {
            if let Some(key) =
                resolve_remote_key_from_agent_auth_profiles(pool, host_id, &fallback).await?
            {
                return Ok(key);
            }
        }
    }

    // 4. Provider env var conventions.
    for env_name in provider_env_var_candidates(&profile.provider) {
        if let Some(key) = read_remote_env_var(pool, host_id, &env_name).await? {
            return Ok(key);
        }
    }

    Ok(String::new())
}

struct RemoteAuthCache {
    env_vars: HashMap<String, String>,
    auth_store_files: Vec<Value>,
}

impl RemoteAuthCache {
    /// Build cache by collecting all needed env var names from all profiles
    /// (including SecretRef env vars from auth stores) and reading them +
    /// all auth-store files in bulk.
    pub(crate) async fn build(
        pool: &SshConnectionPool,
        host_id: &str,
        profiles: &[ModelProfile],
    ) -> Result<Self, String> {
        // Collect env var names needed from profile auth_refs and provider conventions.
        let mut env_var_names = Vec::<String>::new();
        let mut seen_env = std::collections::HashSet::<String>::new();
        for profile in profiles {
            let auth_ref = profile.auth_ref.trim();
            if !auth_ref.is_empty()
                && is_valid_env_var_name(auth_ref)
                && seen_env.insert(auth_ref.to_string())
            {
                env_var_names.push(auth_ref.to_string());
            }
            for env_name in provider_env_var_candidates(&profile.provider) {
                if seen_env.insert(env_name.clone()) {
                    env_var_names.push(env_name);
                }
            }
        }

        // Read all auth-store files from remote agents first so we can
        // discover additional env var names referenced by SecretRefs.
        let auth_store_files = Self::read_auth_store_files(pool, host_id).await?;

        // Scan auth store files for env-source SecretRef references and
        // include their env var names in the batch read.
        for data in &auth_store_files {
            for name in collect_secret_ref_env_names_from_auth_store(data) {
                if seen_env.insert(name.clone()) {
                    env_var_names.push(name);
                }
            }
        }

        // Batch-read all env vars in a single SSH call.
        let env_vars = if env_var_names.is_empty() {
            HashMap::new()
        } else {
            Self::batch_read_env_vars(pool, host_id, &env_var_names).await?
        };

        Ok(Self {
            env_vars,
            auth_store_files,
        })
    }

    pub(crate) async fn batch_read_env_vars(
        pool: &SshConnectionPool,
        host_id: &str,
        names: &[String],
    ) -> Result<HashMap<String, String>, String> {
        // Build a shell script that prints "NAME=VALUE\0" for each set var.
        // Using NUL delimiter avoids issues with newlines in values.
        let mut script = String::from("for __v in");
        for name in names {
            // All names are validated by is_valid_env_var_name, safe to interpolate.
            script.push(' ');
            script.push_str(name);
        }
        script.push_str("; do eval \"__val=\\${$__v+__SET__}\\${$__v}\"; ");
        script.push_str("case \"$__val\" in __SET__*) printf '%s=%s\\n' \"$__v\" \"${__val#__SET__}\";; esac; done");

        let out = pool
            .exec_login(host_id, &script)
            .await
            .map_err(|e| format!("Failed to batch-read remote env vars: {e}"))?;

        let mut map = HashMap::new();
        for line in out.stdout.lines() {
            if let Some(eq_pos) = line.find('=') {
                let key = &line[..eq_pos];
                let val = line[eq_pos + 1..].trim();
                if !val.is_empty() {
                    map.insert(key.to_string(), val.to_string());
                }
            }
        }
        Ok(map)
    }

    pub(crate) async fn read_auth_store_files(
        pool: &SshConnectionPool,
        host_id: &str,
    ) -> Result<Vec<Value>, String> {
        let roots = resolve_remote_openclaw_roots(pool, host_id).await?;
        let mut store_files = Vec::new();

        for root in &roots {
            let agents_path = format!("{}/agents", root.trim_end_matches('/'));
            let entries = match pool.sftp_list(host_id, &agents_path).await {
                Ok(entries) => entries,
                Err(e) if is_remote_missing_path_error(&e) => continue,
                Err(_) => continue,
            };

            for agent in entries.into_iter().filter(|entry| entry.is_dir) {
                let agent_dir =
                    format!("{}/agents/{}/agent", root.trim_end_matches('/'), agent.name);
                for file_name in ["auth-profiles.json", "auth.json"] {
                    let auth_file = format!("{agent_dir}/{file_name}");
                    let text = match pool.sftp_read(host_id, &auth_file).await {
                        Ok(text) => text,
                        Err(_) => continue,
                    };
                    if let Ok(data) = serde_json::from_str::<Value>(&text) {
                        store_files.push(data);
                    }
                }
            }
        }
        Ok(store_files)
    }

    /// Resolve API key for a single profile using cached data.
    pub(crate) fn resolve_for_profile_with_source(
        &self,
        profile: &ModelProfile,
    ) -> Option<(String, ResolvedCredentialSource)> {
        let auth_ref = profile.auth_ref.trim();
        let has_explicit_auth_ref = !auth_ref.is_empty();

        // 1. Explicit auth_ref as env var, then auth store.
        if has_explicit_auth_ref {
            if is_valid_env_var_name(auth_ref) {
                if let Some(val) = self.env_vars.get(auth_ref) {
                    return Some((val.clone(), ResolvedCredentialSource::ExplicitAuthRef));
                }
            }
            if let Some(key) = self.find_in_auth_stores(auth_ref) {
                return Some((key, ResolvedCredentialSource::ExplicitAuthRef));
            }
        }

        // 2. Direct api_key — before fallback auth_ref.
        if let Some(ref key) = profile.api_key {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                return Some((trimmed.to_string(), ResolvedCredentialSource::ManualApiKey));
            }
        }

        // 3. Fallback provider:default auth_ref.
        let provider = profile.provider.trim().to_lowercase();
        if !provider.is_empty() {
            let fallback = format!("{provider}:default");
            let skip = has_explicit_auth_ref && auth_ref == fallback;
            if !skip {
                if let Some(key) = self.find_in_auth_stores(&fallback) {
                    return Some((key, ResolvedCredentialSource::ProviderFallbackAuthRef));
                }
            }
        }

        // 4. Provider env var conventions.
        for env_name in provider_env_var_candidates(&profile.provider) {
            if let Some(val) = self.env_vars.get(&env_name) {
                return Some((val.clone(), ResolvedCredentialSource::ProviderEnvVar));
            }
        }

        None
    }

    pub(crate) fn resolve_for_profile(&self, profile: &ModelProfile) -> String {
        self.resolve_for_profile_with_source(profile)
            .map(|(key, _)| key)
            .unwrap_or_default()
    }

    pub(crate) fn find_in_auth_stores(&self, auth_ref: &str) -> Option<String> {
        let env_lookup = |name: &str| -> Option<String> { self.env_vars.get(name).cloned() };
        for data in &self.auth_store_files {
            if let Some(key) =
                resolve_key_from_auth_store_json_with_env(data, auth_ref, &env_lookup)
            {
                return Some(key);
            }
        }
        None
    }
}
