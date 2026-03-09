use super::*;
use clawpal_core::ssh::diagnostic::{from_any_error, SshIntent, SshStage};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::time::{timeout, Duration};

const SSH_QUALITY_EXCELLENT_MAX_MS: u64 = 250;
const SSH_QUALITY_GOOD_MAX_MS: u64 = 550;
const SSH_QUALITY_FAIR_MAX_MS: u64 = 1100;
const SSH_QUALITY_POOR_MAX_MS: u64 = 1900;
const SSH_PROBE_CONNECT_TIMEOUT_SECS: u64 = 18;
const SSH_PROBE_STAGE_TIMEOUT_SECS: u64 = 15;
const SSH_PROBE_TOTAL_TIMEOUT_SECS: u64 = 45;

#[derive(Clone)]
struct ProbeEmitter {
    app: AppHandle,
    host_id: String,
    request_id: String,
    current_stage: Arc<Mutex<String>>,
}

impl ProbeEmitter {
    fn set_stage(&self, stage: &str) {
        if let Ok(mut guard) = self.current_stage.lock() {
            *guard = stage.to_string();
        }
    }

    fn current_stage(&self) -> String {
        self.current_stage
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| "connect".to_string())
    }

    fn emit(&self, stage: &str, phase: &str, latency_ms: Option<u64>, note: Option<String>) {
        let payload = serde_json::json!({
            "hostId": self.host_id,
            "requestId": self.request_id,
            "stage": stage,
            "phase": phase,
            "latencyMs": latency_ms,
            "note": note,
        });
        let _ = self.app.emit("ssh:probe-progress", payload);
    }
}

fn emit_probe_progress(
    emitter: Option<&ProbeEmitter>,
    stage: &str,
    phase: &str,
    latency_ms: Option<u64>,
    note: Option<String>,
) {
    if let Some(emitter) = emitter {
        emitter.emit(stage, phase, latency_ms, note);
    }
}

fn start_probe_stage(emitter: Option<&ProbeEmitter>, stage: &str) {
    if let Some(emitter) = emitter {
        emitter.set_stage(stage);
        emitter.emit(stage, "start", None, None);
    }
}

fn classify_connection_quality(total_ms: u64) -> (&'static str, u8) {
    match total_ms {
        0..=SSH_QUALITY_EXCELLENT_MAX_MS => ("excellent", 100),
        latency_ms if latency_ms <= SSH_QUALITY_GOOD_MAX_MS => ("good", 84),
        latency_ms if latency_ms <= SSH_QUALITY_FAIR_MAX_MS => ("fair", 66),
        latency_ms if latency_ms <= SSH_QUALITY_POOR_MAX_MS => ("poor", 42),
        _ => ("poor", 18),
    }
}

fn pick_bottleneck_stage(
    connect_ms: u64,
    gateway_ms: u64,
    config_ms: u64,
    version_ms: u64,
    agents_ms: u64,
) -> (&'static str, u64) {
    let samples = [
        ("connect", connect_ms),
        ("gateway", gateway_ms),
        ("config", config_ms),
        ("agents", agents_ms),
        ("version", version_ms),
    ];
    let mut bottleneck = ("other", 0_u64);
    for (stage, latency_ms) in samples {
        if latency_ms > bottleneck.1 {
            bottleneck = (stage, latency_ms);
        }
    }
    bottleneck
}

struct ProbeStageError {
    message: String,
    latency_ms: u64,
}

async fn time_probe_stage<T, F>(
    timeout_secs: u64,
    timeout_message: &'static str,
    future: F,
) -> Result<(T, u64), ProbeStageError>
where
    F: std::future::Future<Output = Result<T, String>>,
{
    let start = Instant::now();
    let output = timeout(Duration::from_secs(timeout_secs), future)
        .await
        .map_err(|_| ProbeStageError {
            message: timeout_message.to_string(),
            latency_ms: start.elapsed().as_millis() as u64,
        })?
        .map_err(|message| ProbeStageError {
            message,
            latency_ms: start.elapsed().as_millis() as u64,
        })?;
    Ok((output, start.elapsed().as_millis() as u64))
}

fn stage_row(key: &str, latency_ms: u64, status: &str, note: Option<String>) -> SshConnectionStage {
    SshConnectionStage {
        key: key.to_string(),
        latency_ms,
        status: status.to_string(),
        note,
    }
}

fn append_not_run_stages(stages: &mut Vec<SshConnectionStage>, after_key: &str) {
    let ordered = ["connect", "gateway", "config", "agents", "version"];
    let Some(idx) = ordered.iter().position(|key| *key == after_key) else {
        return;
    };
    for key in ordered.iter().skip(idx + 1) {
        stages.push(stage_row(key, 0, "not_run", None));
    }
}

fn stage_to_diagnostic_stage(stage_key: &str) -> SshStage {
    match stage_key {
        "connect" => SshStage::SessionOpen,
        "config" => SshStage::SftpRead,
        _ => SshStage::RemoteExec,
    }
}

fn build_probe_profile(
    probe_status: &str,
    reused_existing_connection: bool,
    status: StatusLight,
    connect_latency_ms: u64,
    gateway_latency_ms: u64,
    config_latency_ms: u64,
    agents_latency_ms: u64,
    version_latency_ms: u64,
    total_latency_ms: u64,
    bottleneck_stage: &str,
    bottleneck_latency_ms: u64,
    stages: Vec<SshConnectionStage>,
) -> SshConnectionProfile {
    let (quality, quality_score) = if probe_status == "success" {
        let (quality, quality_score) = classify_connection_quality(total_latency_ms);
        (quality.to_string(), quality_score)
    } else {
        ("unknown".to_string(), 0)
    };

    SshConnectionProfile {
        probe_status: probe_status.to_string(),
        reused_existing_connection,
        status,
        connect_latency_ms,
        gateway_latency_ms,
        config_latency_ms,
        agents_latency_ms,
        version_latency_ms,
        total_latency_ms,
        quality,
        quality_score,
        bottleneck: SshBottleneck {
            stage: bottleneck_stage.to_string(),
            latency_ms: bottleneck_latency_ms,
        },
        stages,
    }
}

fn build_failed_probe_profile(
    reused_existing_connection: bool,
    failing_stage_key: &str,
    connect_latency_ms: u64,
    gateway_latency_ms: u64,
    config_latency_ms: u64,
    agents_latency_ms: u64,
    version_latency_ms: u64,
    total_latency_ms: u64,
    stages: Vec<SshConnectionStage>,
    diagnostic: SshDiagnosticReport,
) -> SshConnectionProfile {
    build_probe_profile(
        "failed",
        reused_existing_connection,
        StatusLight {
            healthy: false,
            active_agents: 0,
            global_default_model: None,
            fallback_models: Vec::new(),
            ssh_diagnostic: Some(diagnostic),
        },
        connect_latency_ms,
        gateway_latency_ms,
        config_latency_ms,
        agents_latency_ms,
        version_latency_ms,
        total_latency_ms,
        failing_stage_key,
        match failing_stage_key {
            "connect" => connect_latency_ms,
            "gateway" => gateway_latency_ms,
            "config" => config_latency_ms,
            "agents" => agents_latency_ms,
            "version" => version_latency_ms,
            _ => 0,
        },
        stages,
    )
}

fn build_interactive_probe_profile(
    connect_latency_ms: u64,
    total_latency_ms: u64,
    stages: Vec<SshConnectionStage>,
    diagnostic: SshDiagnosticReport,
) -> SshConnectionProfile {
    build_probe_profile(
        "interactive_required",
        false,
        StatusLight {
            healthy: false,
            active_agents: 0,
            global_default_model: None,
            fallback_models: Vec::new(),
            ssh_diagnostic: Some(diagnostic),
        },
        connect_latency_ms,
        0,
        0,
        0,
        0,
        total_latency_ms,
        "connect",
        connect_latency_ms,
        stages,
    )
}

async fn connect_host_for_probe(
    pool: &SshConnectionPool,
    host: &SshHostConfig,
) -> Result<(bool, u64), ProbeStageError> {
    if pool.is_connected(&host.id).await {
        return Ok((true, 0));
    }

    let ((), latency_ms) = time_probe_stage(
        SSH_PROBE_CONNECT_TIMEOUT_SECS,
        "ssh connect timed out during probe",
        async {
            if let Some(passphrase) = host.passphrase.as_deref().filter(|value| !value.is_empty()) {
                pool.connect_with_passphrase(host, Some(passphrase)).await
            } else {
                pool.connect(host).await
            }
        },
    )
    .await?;

    Ok((false, latency_ms))
}

async fn probe_ssh_connection_profile_impl(
    pool: &SshConnectionPool,
    host_id: &str,
    emitter: Option<ProbeEmitter>,
) -> Result<SshConnectionProfile, String> {
    let host = read_hosts_from_registry()?
        .into_iter()
        .find(|candidate| candidate.id == host_id)
        .ok_or_else(|| format!("No SSH host config with id: {host_id}"))?;

    let total_start = Instant::now();
    let mut stages = Vec::new();
    let emitter_ref = emitter.as_ref();

    start_probe_stage(emitter_ref, "connect");
    let (reused_existing_connection, connect_latency_ms) =
        match connect_host_for_probe(pool, &host).await {
            Ok(result) => result,
            Err(error) => {
                let diagnostic = from_any_error(
                    SshStage::SessionOpen,
                    SshIntent::Connect,
                    error.message.clone(),
                );
                let connect_latency_ms = error.latency_ms;
                if matches!(
                    diagnostic.error_code,
                    Some(clawpal_core::ssh::diagnostic::SshErrorCode::PassphraseRequired)
                ) {
                    emit_probe_progress(
                        emitter_ref,
                        "connect",
                        "interactive_required",
                        Some(connect_latency_ms),
                        Some(diagnostic.summary.clone()),
                    );
                    stages.push(stage_row(
                        "connect",
                        connect_latency_ms,
                        "interactive_required",
                        Some(diagnostic.summary.clone()),
                    ));
                    append_not_run_stages(&mut stages, "connect");
                    return Ok(build_interactive_probe_profile(
                        connect_latency_ms,
                        total_start.elapsed().as_millis() as u64,
                        stages,
                        diagnostic,
                    ));
                }
                emit_probe_progress(
                    emitter_ref,
                    "connect",
                    "failed",
                    Some(connect_latency_ms),
                    Some(diagnostic.summary.clone()),
                );
                stages.push(stage_row(
                    "connect",
                    connect_latency_ms,
                    "failed",
                    Some(diagnostic.summary.clone()),
                ));
                append_not_run_stages(&mut stages, "connect");
                return Ok(build_failed_probe_profile(
                    false,
                    "connect",
                    connect_latency_ms,
                    0,
                    0,
                    0,
                    0,
                    total_start.elapsed().as_millis() as u64,
                    stages,
                    diagnostic,
                ));
            }
        };

    if reused_existing_connection {
        emit_probe_progress(
            emitter_ref,
            "connect",
            "reused",
            Some(0),
            Some("Session reused".to_string()),
        );
        stages.push(stage_row(
            "connect",
            0,
            "reused",
            Some("Session reused".to_string()),
        ));
    } else {
        emit_probe_progress(
            emitter_ref,
            "connect",
            "success",
            Some(connect_latency_ms),
            None,
        );
        stages.push(stage_row("connect", connect_latency_ms, "ok", None));
    }

    start_probe_stage(emitter_ref, "gateway");
    let (gateway_res, gateway_latency_ms) = match time_probe_stage(
        SSH_PROBE_STAGE_TIMEOUT_SECS,
        "gateway probe timed out",
        async {
            pool.exec_login(host_id, "pgrep -f '[o]penclaw-gateway' >/dev/null 2>&1")
                .await
        },
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            let diagnostic = from_any_error(
                stage_to_diagnostic_stage("gateway"),
                SshIntent::HealthCheck,
                error.message,
            );
            emit_probe_progress(
                emitter_ref,
                "gateway",
                "failed",
                Some(error.latency_ms),
                Some(diagnostic.summary.clone()),
            );
            stages.push(stage_row(
                "gateway",
                error.latency_ms,
                "failed",
                Some(diagnostic.summary.clone()),
            ));
            append_not_run_stages(&mut stages, "gateway");
            return Ok(build_failed_probe_profile(
                reused_existing_connection,
                "gateway",
                connect_latency_ms,
                error.latency_ms,
                0,
                0,
                0,
                total_start.elapsed().as_millis() as u64,
                stages,
                diagnostic,
            ));
        }
    };
    emit_probe_progress(
        emitter_ref,
        "gateway",
        "success",
        Some(gateway_latency_ms),
        None,
    );
    stages.push(stage_row("gateway", gateway_latency_ms, "ok", None));

    start_probe_stage(emitter_ref, "config");
    let ((_, _normalized_raw, config_json), config_latency_ms) = match time_probe_stage(
        SSH_PROBE_STAGE_TIMEOUT_SECS,
        "config probe timed out",
        async { remote_read_openclaw_config_text_and_json(pool, host_id).await },
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            let diagnostic = from_any_error(
                stage_to_diagnostic_stage("config"),
                SshIntent::HealthCheck,
                error.message,
            );
            emit_probe_progress(
                emitter_ref,
                "config",
                "failed",
                Some(error.latency_ms),
                Some(diagnostic.summary.clone()),
            );
            stages.push(stage_row(
                "config",
                error.latency_ms,
                "failed",
                Some(diagnostic.summary.clone()),
            ));
            append_not_run_stages(&mut stages, "config");
            return Ok(build_failed_probe_profile(
                reused_existing_connection,
                "config",
                connect_latency_ms,
                gateway_latency_ms,
                error.latency_ms,
                0,
                0,
                total_start.elapsed().as_millis() as u64,
                stages,
                diagnostic,
            ));
        }
    };
    emit_probe_progress(
        emitter_ref,
        "config",
        "success",
        Some(config_latency_ms),
        None,
    );
    stages.push(stage_row("config", config_latency_ms, "ok", None));

    start_probe_stage(emitter_ref, "agents");
    let (agents_res, agents_latency_ms) = match time_probe_stage(
        SSH_PROBE_STAGE_TIMEOUT_SECS,
        "agents probe timed out",
        async {
            pool.exec_login(host_id, "openclaw agents list --json")
                .await
        },
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            let diagnostic = from_any_error(
                stage_to_diagnostic_stage("agents"),
                SshIntent::HealthCheck,
                error.message,
            );
            emit_probe_progress(
                emitter_ref,
                "agents",
                "failed",
                Some(error.latency_ms),
                Some(diagnostic.summary.clone()),
            );
            stages.push(stage_row(
                "agents",
                error.latency_ms,
                "failed",
                Some(diagnostic.summary.clone()),
            ));
            append_not_run_stages(&mut stages, "agents");
            return Ok(build_failed_probe_profile(
                reused_existing_connection,
                "agents",
                connect_latency_ms,
                gateway_latency_ms,
                config_latency_ms,
                error.latency_ms,
                0,
                total_start.elapsed().as_millis() as u64,
                stages,
                diagnostic,
            ));
        }
    };
    emit_probe_progress(
        emitter_ref,
        "agents",
        "success",
        Some(agents_latency_ms),
        None,
    );
    stages.push(stage_row("agents", agents_latency_ms, "ok", None));

    start_probe_stage(emitter_ref, "version");
    let (version_res, version_latency_ms) = match time_probe_stage(
        SSH_PROBE_STAGE_TIMEOUT_SECS,
        "version probe timed out",
        async { pool.exec_login(host_id, "openclaw --version").await },
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            let diagnostic = from_any_error(
                stage_to_diagnostic_stage("version"),
                SshIntent::HealthCheck,
                error.message,
            );
            emit_probe_progress(
                emitter_ref,
                "version",
                "failed",
                Some(error.latency_ms),
                Some(diagnostic.summary.clone()),
            );
            stages.push(stage_row(
                "version",
                error.latency_ms,
                "failed",
                Some(diagnostic.summary.clone()),
            ));
            return Ok(build_failed_probe_profile(
                reused_existing_connection,
                "version",
                connect_latency_ms,
                gateway_latency_ms,
                config_latency_ms,
                agents_latency_ms,
                error.latency_ms,
                total_start.elapsed().as_millis() as u64,
                stages,
                diagnostic,
            ));
        }
    };
    emit_probe_progress(
        emitter_ref,
        "version",
        "success",
        Some(version_latency_ms),
        None,
    );
    stages.push(stage_row("version", version_latency_ms, "ok", None));

    let active_agents = if agents_res.exit_code == 0 {
        let json = serde_json::from_str::<Value>(&agents_res.stdout).unwrap_or(Value::Null);
        count_agent_entries_from_cli_json(&json).unwrap_or(0)
    } else {
        0
    };

    let global_default_model = config_json
        .pointer("/defaults/model")
        .and_then(read_model_value)
        .or_else(|| {
            config_json
                .pointer("/default/model")
                .and_then(read_model_value)
        });
    let fallback_models = config_json
        .pointer("/defaults/model/fallbacks")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    let _openclaw_version = {
        let trimmed = version_res.stdout.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    };

    let healthy = gateway_res.exit_code == 0 || config_json != Value::Null;
    let total_latency_ms = total_start.elapsed().as_millis() as u64;
    let (bottleneck_stage, bottleneck_latency_ms) = pick_bottleneck_stage(
        connect_latency_ms,
        gateway_latency_ms,
        config_latency_ms,
        version_latency_ms,
        agents_latency_ms,
    );
    emit_probe_progress(
        emitter_ref,
        "version",
        "completed",
        Some(total_latency_ms),
        None,
    );

    Ok(build_probe_profile(
        "success",
        reused_existing_connection,
        StatusLight {
            healthy,
            active_agents,
            global_default_model,
            fallback_models,
            ssh_diagnostic: None,
        },
        connect_latency_ms,
        gateway_latency_ms,
        config_latency_ms,
        agents_latency_ms,
        version_latency_ms,
        total_latency_ms,
        bottleneck_stage,
        bottleneck_latency_ms,
        stages,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_connection_quality_respects_tuned_thresholds() {
        assert_eq!(classify_connection_quality(0), ("excellent", 100));
        assert_eq!(
            classify_connection_quality(SSH_QUALITY_EXCELLENT_MAX_MS),
            ("excellent", 100)
        );
        assert_eq!(
            classify_connection_quality(SSH_QUALITY_EXCELLENT_MAX_MS + 1),
            ("good", 84)
        );
        assert_eq!(
            classify_connection_quality(SSH_QUALITY_GOOD_MAX_MS),
            ("good", 84)
        );
        assert_eq!(
            classify_connection_quality(SSH_QUALITY_GOOD_MAX_MS + 1),
            ("fair", 66)
        );
        assert_eq!(
            classify_connection_quality(SSH_QUALITY_FAIR_MAX_MS),
            ("fair", 66)
        );
        assert_eq!(
            classify_connection_quality(SSH_QUALITY_FAIR_MAX_MS + 1),
            ("poor", 42)
        );
        assert_eq!(
            classify_connection_quality(SSH_QUALITY_POOR_MAX_MS),
            ("poor", 42)
        );
        assert_eq!(
            classify_connection_quality(SSH_QUALITY_POOR_MAX_MS + 1),
            ("poor", 18)
        );
    }

    #[test]
    fn pick_bottleneck_stage_prefers_largest_latency() {
        let (stage, latency) = pick_bottleneck_stage(120, 90, 400, 250, 100);
        assert_eq!((stage, latency), ("config", 400));
    }

    #[test]
    fn pick_bottleneck_stage_keeps_other_on_empty_measurements() {
        let (stage, latency) = pick_bottleneck_stage(0, 0, 0, 0, 0);
        assert_eq!((stage, latency), ("other", 0));
    }

    #[test]
    fn pick_bottleneck_stage_includes_agents_stage() {
        let (stage, latency) = pick_bottleneck_stage(120, 90, 200, 250, 480);
        assert_eq!((stage, latency), ("agents", 480));
    }

    #[test]
    fn count_agent_entries_from_cli_json_uses_real_list_length() {
        let json = serde_json::json!([
            { "id": "main" },
            { "id": "agent-2" },
            { "id": "agent-3" },
            { "id": "agent-4" }
        ]);

        assert_eq!(count_agent_entries_from_cli_json(&json).unwrap(), 4);
    }

    #[test]
    fn count_agent_entries_from_cli_json_keeps_empty_lists_empty() {
        let json = serde_json::json!([]);

        assert_eq!(count_agent_entries_from_cli_json(&json).unwrap(), 0);
    }
}

#[tauri::command]
pub async fn remote_run_doctor(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Value, String> {
    let result = pool
        .exec_login(
            &host_id,
            "openclaw doctor --json 2>/dev/null || openclaw doctor 2>&1",
        )
        .await?;
    // Try to parse as JSON first
    if let Ok(json) = serde_json::from_str::<Value>(&result.stdout) {
        return Ok(json);
    }
    // Fallback: return raw output as a simple report
    Ok(serde_json::json!({
        "ok": result.exit_code == 0,
        "score": if result.exit_code == 0 { 100 } else { 0 },
        "issues": [],
        "rawOutput": result.stdout,
    }))
}

#[tauri::command]
pub async fn remote_fix_issues(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    ids: Vec<String>,
) -> Result<FixResult, String> {
    let (config_path, raw, _cfg) =
        remote_read_openclaw_config_text_and_json(&pool, &host_id).await?;
    let mut cfg = clawpal_core::doctor::parse_json5_document_or_default(&raw);
    let applied = clawpal_core::doctor::apply_issue_fixes(&mut cfg, &ids)?;

    if !applied.is_empty() {
        remote_write_config_with_snapshot(&pool, &host_id, &config_path, &raw, &cfg, "doctor-fix")
            .await?;
    }

    let remaining: Vec<String> = ids.into_iter().filter(|id| !applied.contains(id)).collect();
    Ok(FixResult {
        ok: true,
        applied,
        remaining_issues: remaining,
    })
}

#[tauri::command]
pub async fn remote_get_system_status(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<StatusLight, String> {
    // Tier 1: fast, essential — health check + config + real agent list.
    let (config_res, agents_res, pgrep_res) = tokio::join!(
        run_openclaw_remote_with_autofix(&pool, &host_id, &["config", "get", "agents", "--json"]),
        run_openclaw_remote_with_autofix(&pool, &host_id, &["agents", "list", "--json"]),
        pool.exec(&host_id, "pgrep -f '[o]penclaw-gateway' >/dev/null 2>&1"),
    );

    let config_ok = matches!(&config_res, Ok(output) if output.exit_code == 0);
    let ssh_diagnostic = match (&config_res, &agents_res, &pgrep_res) {
        (Err(error), _, _) => Some(from_any_error(
            SshStage::RemoteExec,
            SshIntent::HealthCheck,
            error.clone(),
        )),
        (_, Err(error), _) => Some(from_any_error(
            SshStage::RemoteExec,
            SshIntent::HealthCheck,
            error.clone(),
        )),
        (_, _, Err(error)) => Some(from_any_error(
            SshStage::RemoteExec,
            SshIntent::HealthCheck,
            error.clone(),
        )),
        _ => None,
    };

    let active_agents = match &agents_res {
        Ok(output) if output.exit_code == 0 => {
            let json = crate::cli_runner::parse_json_output(output).unwrap_or(Value::Null);
            count_agent_entries_from_cli_json(&json).unwrap_or(0)
        }
        _ => 0,
    };

    let (global_default_model, fallback_models) = match config_res {
        Ok(ref output) if output.exit_code == 0 => {
            let cfg: Value = crate::cli_runner::parse_json_output(output).unwrap_or(Value::Null);
            let model = cfg
                .pointer("/defaults/model")
                .and_then(|v| read_model_value(v))
                .or_else(|| {
                    cfg.pointer("/default/model")
                        .and_then(|v| read_model_value(v))
                });
            let fallbacks = cfg
                .pointer("/defaults/model/fallbacks")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            (model, fallbacks)
        }
        _ => (None, Vec::new()),
    };

    // Avoid false negatives from transient SSH exec failures:
    // if health probe fails but config fetch in the same cycle succeeded,
    // keep health as true instead of flipping to unhealthy.
    let healthy = match pgrep_res {
        Ok(r) => r.exit_code == 0,
        Err(_) if config_ok => true,
        Err(_) => false,
    };

    Ok(StatusLight {
        healthy,
        active_agents,
        global_default_model,
        fallback_models,
        ssh_diagnostic,
    })
}

#[tauri::command]
pub async fn probe_ssh_connection_profile(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    request_id: String,
    app: AppHandle,
) -> Result<SshConnectionProfile, String> {
    let emitter = ProbeEmitter {
        app,
        host_id: host_id.clone(),
        request_id,
        current_stage: Arc::new(Mutex::new("connect".to_string())),
    };

    match timeout(
        Duration::from_secs(SSH_PROBE_TOTAL_TIMEOUT_SECS),
        probe_ssh_connection_profile_impl(&pool, &host_id, Some(emitter.clone())),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            let current_stage = emitter.current_stage();
            let message = format!("ssh probe timed out during {current_stage}");
            emitter.emit(&current_stage, "failed", None, Some(message.clone()));
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn remote_get_ssh_connection_profile(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<SshConnectionProfile, String> {
    timeout(
        Duration::from_secs(SSH_PROBE_TOTAL_TIMEOUT_SECS),
        probe_ssh_connection_profile_impl(&pool, &host_id, None),
    )
    .await
    .map_err(|_| "ssh probe timed out".to_string())?
}

#[tauri::command]
pub async fn remote_get_status_extra(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<StatusExtra, String> {
    let detect_duplicates_script = concat!(
        "seen=''; for p in $(which -a openclaw 2>/dev/null) ",
        "\"$HOME/.npm-global/bin/openclaw\" \"/usr/local/bin/openclaw\" \"/opt/homebrew/bin/openclaw\"; do ",
        "[ -x \"$p\" ] || continue; ",
        "rp=$(readlink -f \"$p\" 2>/dev/null || echo \"$p\"); ",
        "echo \"$seen\" | grep -qF \"$rp\" && continue; ",
        "seen=\"$seen $rp\"; ",
        "v=$($p --version 2>/dev/null || echo 'unknown'); ",
        "echo \"$p: $v\"; ",
        "done"
    );

    let (version_res, dup_res) = tokio::join!(
        pool.exec_login(&host_id, "openclaw --version"),
        pool.exec_login(&host_id, detect_duplicates_script),
    );

    let openclaw_version = match version_res {
        Ok(r) if r.exit_code == 0 => Some(r.stdout.trim().to_string()),
        Ok(r) => {
            let trimmed = r.stdout.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Err(_) => None,
    };

    let duplicate_installs = match dup_res {
        Ok(r) => {
            let entries: Vec<String> = r
                .stdout
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            if entries.len() > 1 {
                entries
            } else {
                Vec::new()
            }
        }
        Err(_) => Vec::new(),
    };

    Ok(StatusExtra {
        openclaw_version,
        duplicate_installs,
    })
}

#[tauri::command]
pub async fn get_status_light() -> Result<StatusLight, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let paths = resolve_paths();
        let cfg = read_openclaw_config(&paths)?;
        let local_health = clawpal_core::health::check_instance(&local_health_instance())
            .map_err(|e| e.to_string())?;
        let active_agents = crate::cli_runner::run_openclaw(&["agents", "list", "--json"])
            .ok()
            .and_then(|output| crate::cli_runner::parse_json_output(&output).ok())
            .and_then(|json| count_agent_entries_from_cli_json(&json).ok())
            .unwrap_or(0);
        let global_default_model = cfg
            .pointer("/agents/defaults/model")
            .and_then(read_model_value)
            .or_else(|| {
                cfg.pointer("/agents/default/model")
                    .and_then(read_model_value)
            });

        let fallback_models = cfg
            .pointer("/agents/defaults/model/fallbacks")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        Ok(StatusLight {
            healthy: local_health.healthy,
            active_agents,
            global_default_model,
            fallback_models,
            ssh_diagnostic: None,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_status_extra() -> Result<StatusExtra, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let openclaw_version = {
            let mut cache = OPENCLAW_VERSION_CACHE.lock().unwrap();
            if cache.is_none() {
                let version = clawpal_core::health::check_instance(&local_health_instance())
                    .ok()
                    .and_then(|status| status.version);
                *cache = Some(version);
            }
            cache.as_ref().unwrap().clone()
        };
        Ok(StatusExtra {
            openclaw_version,
            duplicate_installs: Vec::new(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_system_status() -> Result<SystemStatus, String> {
    let paths = resolve_paths();
    ensure_dirs(&paths)?;
    let cfg = read_openclaw_config(&paths)?;
    let active_agents = cfg
        .get("agents")
        .and_then(|a| a.get("list"))
        .and_then(|a| a.as_array())
        .map(|a| a.len() as u32)
        .unwrap_or(0);
    let snapshots = list_snapshots(&paths.metadata_path)
        .unwrap_or_default()
        .items
        .len();
    let model_summary = collect_model_summary(&cfg);
    let channel_summary = collect_channel_summary(&cfg);
    let memory = collect_memory_overview(&paths.base_dir);
    let sessions = collect_session_overview(&paths.base_dir);
    let openclaw_version = resolve_openclaw_version();
    let openclaw_update =
        check_openclaw_update_cached(&paths, false).unwrap_or_else(|_| OpenclawUpdateCheck {
            installed_version: openclaw_version.clone(),
            latest_version: None,
            upgrade_available: false,
            channel: None,
            details: Some("update status unavailable".into()),
            source: "unknown".into(),
            checked_at: format_timestamp_from_unix(unix_timestamp_secs()),
        });
    Ok(SystemStatus {
        healthy: true,
        config_path: paths.config_path.to_string_lossy().to_string(),
        openclaw_dir: paths.openclaw_dir.to_string_lossy().to_string(),
        clawpal_dir: paths.clawpal_dir.to_string_lossy().to_string(),
        openclaw_version,
        active_agents,
        snapshots,
        channels: channel_summary,
        models: model_summary,
        memory,
        sessions,
        openclaw_update,
    })
}

#[tauri::command]
pub fn run_doctor_command() -> Result<DoctorReport, String> {
    let paths = resolve_paths();
    Ok(run_doctor(&paths))
}

#[tauri::command]
pub fn fix_issues(ids: Vec<String>) -> Result<FixResult, String> {
    let paths = resolve_paths();
    let issues = run_doctor(&paths);
    let mut fixable = Vec::new();
    for issue in issues.issues {
        if ids.contains(&issue.id) && issue.auto_fixable {
            fixable.push(issue.id);
        }
    }
    let auto_applied = apply_auto_fixes(&paths, &fixable);
    let mut remaining = Vec::new();
    let mut applied = Vec::new();
    for id in ids {
        if fixable.contains(&id) && auto_applied.iter().any(|x| x == &id) {
            applied.push(id);
        } else {
            remaining.push(id);
        }
    }
    Ok(FixResult {
        ok: true,
        applied,
        remaining_issues: remaining,
    })
}
