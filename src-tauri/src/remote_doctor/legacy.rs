use std::time::Instant;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Runtime};

use super::config::{
    append_diagnosis_log,
    build_gateway_credentials as remote_doctor_gateway_credentials,
    diagnosis_missing_rescue_profile, diagnosis_unhealthy_rescue_gateway,
    primary_remote_target_host_id, remote_target_host_id_candidates, run_rescue_diagnosis,
};
use super::agent::{
    build_agent_plan_prompt, remote_doctor_agent_id, remote_doctor_agent_session_key,
};
use super::plan::{
    apply_config_set, apply_config_unset, config_read_response, decode_base64_config_payload,
    execute_clawpal_command, execute_command, execute_invoke_payload, parse_invoke_argv,
    parse_plan_response, plan_command_uses_internal_clawpal_tool, request_plan, build_shell_command,
    shell_escape, validate_clawpal_exec_args, validate_plan_command_argv,
};
use super::session::{append_session_log as append_remote_doctor_log, emit_session_progress as emit_progress};
use super::types::{
    CommandResult, ConfigExcerptContext, PlanCommand, PlanKind, PlanResponse, TargetLocation,
};
use crate::bridge_client::BridgeClient;
use crate::commands::{manage_rescue_bot, remote_manage_rescue_bot, RescuePrimaryDiagnosisResult};
use crate::node_client::NodeClient;
use crate::ssh::SshConnectionPool;

pub(crate) async fn ensure_agent_bridge_connected<R: Runtime>(
    app: &AppHandle<R>,
    bridge: &BridgeClient,
    gateway_url: &str,
    auth_token_override: Option<&str>,
    session_id: &str,
) {
    if bridge.is_connected().await {
        return;
    }

    let connect_result = bridge
        .connect(
            gateway_url,
            app.clone(),
            remote_doctor_gateway_credentials(auth_token_override)
                .ok()
                .flatten(),
        )
        .await;
    if let Err(error) = connect_result {
        append_remote_doctor_log(
            session_id,
            json!({
                "event": "bridge_connect_failed",
                "reason": error,
            }),
        );
    }
}

pub(crate) async fn ensure_remote_target_connected(
    pool: &SshConnectionPool,
    instance_id: &str,
) -> Result<(), String> {
    let candidate_ids = remote_target_host_id_candidates(instance_id);
    if candidate_ids.is_empty() {
        return Ok(());
    }
    for candidate in &candidate_ids {
        if pool.is_connected(candidate).await {
            return Ok(());
        }
    }

    let hosts = crate::commands::ssh::read_hosts_from_registry()?;
    let host = hosts
        .into_iter()
        .find(|candidate| candidate_ids.iter().any(|id| id == &candidate.id))
        .ok_or_else(|| format!("No SSH host config with id: {}", candidate_ids[0]))?;
    if let Some(passphrase) = host.passphrase.as_deref().filter(|value| !value.is_empty()) {
        pool.connect_with_passphrase(&host, Some(passphrase)).await
    } else {
        pool.connect(&host).await
    }
}

fn rescue_setup_command_result(
    action: &str,
    profile: &str,
    configured: bool,
    active: bool,
    runtime_state: &str,
) -> CommandResult {
    CommandResult {
        argv: vec!["manage_rescue_bot".into(), action.into(), profile.into()],
        exit_code: Some(0),
        stdout: format!(
            "configured={} active={} runtimeState={}",
            configured, active, runtime_state
        ),
        stderr: String::new(),
        duration_ms: 0,
        timed_out: false,
    }
}

fn rescue_bot_manage_command_result(
    result: &crate::commands::RescueBotManageResult,
) -> CommandResult {
    CommandResult {
        argv: vec![
            "manage_rescue_bot".into(),
            result.action.clone(),
            result.profile.clone(),
        ],
        exit_code: Some(if result.active || result.configured {
            0
        } else {
            1
        }),
        stdout: format!(
            "configured={} active={} runtimeState={} rescuePort={} mainPort={} commands={}",
            result.configured,
            result.active,
            result.runtime_state,
            result.rescue_port,
            result.main_port,
            result.commands.len()
        ),
        stderr: String::new(),
        duration_ms: 0,
        timed_out: false,
    }
}

fn rescue_activation_diagnostic_commands(profile: &str) -> Vec<Vec<String>> {
    vec![
        vec!["manage_rescue_bot".into(), "status".into(), profile.into()],
        vec![
            "openclaw".into(),
            "--profile".into(),
            profile.into(),
            "gateway".into(),
            "status".into(),
        ],
        vec![
            "openclaw".into(),
            "--profile".into(),
            profile.into(),
            "config".into(),
            "get".into(),
            "gateway.port".into(),
            "--json".into(),
        ],
    ]
}

fn rescue_activation_error_message(
    profile: &str,
    configured: bool,
    runtime_state: &str,
    suggested_checks: &[String],
) -> String {
    let suffix = if suggested_checks.is_empty() {
        String::new()
    } else {
        format!(" Suggested checks: {}.", suggested_checks.join("; "))
    };
    format!(
        "Rescue profile \"{}\" was {} but did not become active (runtime state: {}).",
        profile,
        if configured {
            "configured"
        } else {
            "not configured"
        },
        runtime_state
    ) + &suffix
}

async fn execute_rescue_activation_diagnostic_command<R: Runtime>(
    app: &AppHandle<R>,
    target_location: TargetLocation,
    instance_id: &str,
    argv: &[String],
) -> CommandResult {
    let started = Instant::now();
    if argv.first().map(String::as_str) == Some("manage_rescue_bot")
        && argv.get(1).map(String::as_str) == Some("status")
    {
        let profile = argv
            .get(2)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("rescue");
        let result = match target_location {
            TargetLocation::LocalOpenclaw => {
                manage_rescue_bot("status".into(), Some(profile.to_string()), None).await
            }
            TargetLocation::RemoteOpenclaw => {
                let host_id = primary_remote_target_host_id(instance_id);
                match host_id {
                    Ok(host_id) => {
                        remote_manage_rescue_bot(
                            app.state::<SshConnectionPool>(),
                            host_id,
                            "status".into(),
                            Some(profile.to_string()),
                            None,
                        )
                        .await
                    }
                    Err(error) => Err(error),
                }
            }
        };
        return match result {
            Ok(result) => {
                let mut command_result = rescue_bot_manage_command_result(&result);
                command_result.duration_ms = started.elapsed().as_millis() as u64;
                command_result
            }
            Err(error) => CommandResult {
                argv: argv.to_vec(),
                exit_code: Some(1),
                stdout: String::new(),
                stderr: error,
                duration_ms: started.elapsed().as_millis() as u64,
                timed_out: false,
            },
        };
    }

    match execute_command(
        &app.state::<SshConnectionPool>(),
        target_location,
        instance_id,
        argv,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => CommandResult {
            argv: argv.to_vec(),
            exit_code: Some(1),
            stdout: String::new(),
            stderr: error,
            duration_ms: started.elapsed().as_millis() as u64,
            timed_out: false,
        },
    }
}

async fn collect_rescue_activation_failure_diagnostics<R: Runtime>(
    app: &AppHandle<R>,
    target_location: TargetLocation,
    instance_id: &str,
    profile: &str,
) -> Vec<CommandResult> {
    let mut results = Vec::new();
    for argv in rescue_activation_diagnostic_commands(profile) {
        results.push(
            execute_rescue_activation_diagnostic_command(app, target_location, instance_id, &argv)
                .await,
        );
    }
    results
}

struct RescueActivationFailure {
    message: String,
    activation_result: CommandResult,
    diagnostics: Vec<CommandResult>,
}

async fn ensure_rescue_profile_ready<R: Runtime>(
    app: &AppHandle<R>,
    target_location: TargetLocation,
    instance_id: &str,
) -> Result<CommandResult, RescueActivationFailure> {
    let started = Instant::now();
    let result = match target_location {
        TargetLocation::LocalOpenclaw => {
            manage_rescue_bot("activate".into(), Some("rescue".into()), None)
                .await
                .map_err(|error| RescueActivationFailure {
                    message: error,
                    activation_result: rescue_setup_command_result(
                        "activate",
                        "rescue",
                        false,
                        false,
                        "activation_failed",
                    ),
                    diagnostics: Vec::new(),
                })?
        }
        TargetLocation::RemoteOpenclaw => {
            let host_id = primary_remote_target_host_id(instance_id).map_err(|error| {
                RescueActivationFailure {
                    message: error,
                    activation_result: rescue_setup_command_result(
                        "activate",
                        "rescue",
                        false,
                        false,
                        "activation_failed",
                    ),
                    diagnostics: Vec::new(),
                }
            })?;
            remote_manage_rescue_bot(
                app.state::<SshConnectionPool>(),
                host_id,
                "activate".into(),
                Some("rescue".into()),
                None,
            )
            .await
            .map_err(|error| RescueActivationFailure {
                message: error,
                activation_result: rescue_setup_command_result(
                    "activate",
                    "rescue",
                    false,
                    false,
                    "activation_failed",
                ),
                diagnostics: Vec::new(),
            })?
        }
    };
    let mut command_result = rescue_setup_command_result(
        &result.action,
        &result.profile,
        result.configured,
        result.active,
        &result.runtime_state,
    );
    command_result.duration_ms = started.elapsed().as_millis() as u64;
    if !result.active {
        let diagnostics = collect_rescue_activation_failure_diagnostics(
            app,
            target_location,
            instance_id,
            &result.profile,
        )
        .await;
        let suggested_checks = diagnostics
            .iter()
            .map(|result| result.argv.join(" "))
            .collect::<Vec<_>>();
        return Err(RescueActivationFailure {
            message: rescue_activation_error_message(
                &result.profile,
                result.configured,
                &result.runtime_state,
                &suggested_checks,
            ),
            activation_result: command_result,
            diagnostics,
        });
    }
    Ok(command_result)
}

pub(crate) async fn repair_rescue_gateway_if_needed<R: Runtime>(
    app: &AppHandle<R>,
    session_id: &str,
    round: usize,
    target_location: TargetLocation,
    instance_id: &str,
    diagnosis: &mut RescuePrimaryDiagnosisResult,
) -> Result<(), String> {
    if !(diagnosis_missing_rescue_profile(diagnosis)
        || diagnosis_unhealthy_rescue_gateway(diagnosis))
    {
        return Ok(());
    }

    emit_progress(
        Some(app),
        session_id,
        round,
        "preparing_rescue",
        "Activating rescue profile before requesting remote repair plan",
        Some(PlanKind::Repair),
        None,
    );
    let setup_result = match ensure_rescue_profile_ready(app, target_location, instance_id).await {
        Ok(setup_result) => setup_result,
        Err(failure) => {
            append_remote_doctor_log(
                session_id,
                json!({
                    "event": "rescue_profile_activation",
                    "round": round,
                    "result": failure.activation_result,
                    "status": "failed",
                }),
            );
            append_remote_doctor_log(
                session_id,
                json!({
                    "event": "rescue_activation_diagnosis",
                    "round": round,
                    "checks": failure.diagnostics,
                }),
            );
            return Err(failure.message);
        }
    };
    append_remote_doctor_log(
        session_id,
        json!({
            "event": "rescue_profile_activation",
            "round": round,
            "result": setup_result,
        }),
    );
    *diagnosis = run_rescue_diagnosis(app, target_location, instance_id).await?;
    append_diagnosis_log(session_id, "after_rescue_activation", round, diagnosis);
    Ok(())
}

fn extract_json_block(text: &str) -> Option<&str> {
    clawpal_core::doctor::extract_json_from_output(text)
}

pub(crate) fn parse_agent_plan_response(kind: PlanKind, text: &str) -> Result<PlanResponse, String> {
    let json_block = extract_json_block(text)
        .ok_or_else(|| format!("Remote doctor agent did not return JSON: {text}"))?;
    let value: Value = serde_json::from_str(json_block)
        .map_err(|error| format!("Failed to parse remote doctor agent JSON: {error}"))?;
    parse_plan_response(kind, value)
}

pub(crate) async fn run_agent_request_with_bridge<R: Runtime>(
    app: &AppHandle<R>,
    client: &NodeClient,
    bridge: &BridgeClient,
    pool: &SshConnectionPool,
    target_location: TargetLocation,
    instance_id: &str,
    agent_id: &str,
    session_key: &str,
    message: &str,
) -> Result<String, String> {
    let final_rx = client
        .start_agent_request(agent_id, session_key, message)
        .await?;
    let mut invokes = bridge.subscribe_invokes();
    let final_future = async move {
        final_rx.await.map_err(|_| {
            "Agent request ended before a final chat response was received".to_string()
        })
    };
    tokio::pin!(final_future);

    loop {
        tokio::select! {
            result = &mut final_future => {
                return result;
            }
            event = invokes.recv() => {
                let payload = match event {
                    Ok(payload) => payload,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Err("Bridge invoke stream closed during agent request".into());
                    }
                };
                let invoke_id = payload.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                let node_id = payload.get("nodeId").and_then(Value::as_str).unwrap_or("").to_string();
                let result = execute_invoke_payload(app, pool, target_location, instance_id, &payload).await;
                match result {
                    Ok(value) => {
                        bridge.send_invoke_result(&invoke_id, &node_id, value).await?;
                    }
                    Err(error) => {
                        bridge.send_invoke_error(&invoke_id, &node_id, "EXEC_ERROR", &error).await?;
                    }
                }
                let _ = bridge.take_invoke(&invoke_id).await;
            }
        }
    }
}

pub(crate) async fn request_agent_plan<R: Runtime>(
    app: &AppHandle<R>,
    client: &NodeClient,
    bridge_client: &BridgeClient,
    pool: &SshConnectionPool,
    session_id: &str,
    round: usize,
    kind: PlanKind,
    target_location: TargetLocation,
    instance_id: &str,
    diagnosis: &RescuePrimaryDiagnosisResult,
    config_context: &ConfigExcerptContext,
    previous_results: &[CommandResult],
) -> Result<PlanResponse, String> {
    let agent_session_key = remote_doctor_agent_session_key(session_id);
    let prompt = build_agent_plan_prompt(
        kind,
        session_id,
        round,
        target_location,
        instance_id,
        diagnosis,
        config_context,
        previous_results,
    );
    let text = if bridge_client.is_connected().await {
        run_agent_request_with_bridge(
            app,
            client,
            bridge_client,
            pool,
            target_location,
            instance_id,
            remote_doctor_agent_id(),
            &agent_session_key,
            &prompt,
        )
        .await?
    } else {
        client
            .run_agent_request(remote_doctor_agent_id(), &agent_session_key, &prompt)
            .await?
    };
    parse_agent_plan_response(kind, &text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::create_dir_all;
    use std::io::Write;
    use std::process::Command;

    use uuid::Uuid;

    use crate::remote_doctor::agent::{
        default_remote_doctor_protocol, detect_method_name,
        ensure_agent_workspace_ready as ensure_local_remote_doctor_agent_ready,
        next_agent_plan_kind, next_agent_plan_kind_for_round, protocol_requires_bridge,
        protocol_runs_rescue_preflight,
    };
    use crate::remote_doctor::config::{
        build_config_excerpt_context, config_excerpt_log_summary,
        diagnosis_has_only_non_auto_fixable_issues, empty_config_excerpt_context,
        load_gateway_config as remote_doctor_gateway_config,
    };
    use crate::remote_doctor::plan::plan_command_failure_message;
    use crate::remote_doctor::repair_loops::{
        round_limit_error_message as repair_loops_round_limit_error_message,
        repair_plan_stalled as repair_loops_repair_plan_stalled,
        run_clawpal_server_repair_loop as repair_loops_run_clawpal_server_repair_loop,
        run_remote_doctor_repair_loop as repair_loops_run_remote_doctor_repair_loop,
        start_remote_doctor_repair_impl as repair_loops_start_remote_doctor_repair_impl,
    };
    use crate::remote_doctor::types::{
        diagnosis_issue_summaries, parse_target_location, RemoteDoctorProtocol,
        RepairRoundObservation,
    };
    use crate::cli_runner::{set_active_clawpal_data_override, set_active_openclaw_home_override};
    use crate::ssh::SshHostConfig;
    use std::net::TcpStream;
    use tauri::test::mock_app;

    #[test]
    fn build_shell_command_escapes_single_quotes() {
        let command = build_shell_command(&["echo".into(), "a'b".into()]);
        assert_eq!(command, "'echo' 'a'\\''b'");
    }

    #[test]
    fn parse_target_location_rejects_unknown_values() {
        let error = parse_target_location("elsewhere").unwrap_err();
        assert!(error.contains("Unsupported target location"));
    }

    #[test]
    fn apply_config_set_creates_missing_object_path() {
        let mut value = json!({});
        apply_config_set(
            &mut value,
            "models.providers.openai.baseUrl",
            json!("http://127.0.0.1:3000/v1"),
        )
        .expect("config set");
        assert_eq!(
            value
                .pointer("/models/providers/openai/baseUrl")
                .and_then(Value::as_str),
            Some("http://127.0.0.1:3000/v1")
        );
    }

    #[test]
    fn apply_config_unset_removes_existing_leaf() {
        let mut value = json!({
            "models": {
                "providers": {
                    "openai": {
                        "baseUrl": "http://127.0.0.1:3000/v1",
                        "models": [{"id": "gpt-4.1"}]
                    }
                }
            }
        });
        apply_config_unset(&mut value, "models.providers.openai.baseUrl").expect("config unset");
        assert!(value.pointer("/models/providers/openai/baseUrl").is_none());
        assert!(value.pointer("/models/providers/openai/models").is_some());
    }

    #[test]
    fn parse_agent_plan_response_reads_json_payload() {
        let text = r#"preface
{"planId":"detect-1","planKind":"detect","summary":"ok","commands":[{"argv":["openclaw","doctor","--json"]}],"healthy":false,"done":false,"success":false}
"#;
        let plan = parse_agent_plan_response(PlanKind::Detect, text).expect("parse plan");
        assert_eq!(plan.plan_id, "detect-1");
        assert_eq!(plan.commands[0].argv, vec!["openclaw", "doctor", "--json"]);
    }

    #[test]
    fn build_agent_plan_prompt_mentions_target_and_schema() {
        let prompt = build_agent_plan_prompt(
            PlanKind::Repair,
            "sess-1",
            3,
            TargetLocation::RemoteOpenclaw,
            "ssh:vm1",
            &sample_diagnosis(Vec::new()),
            &ConfigExcerptContext {
                config_excerpt: json!({"ok": true}),
                config_excerpt_raw: None,
                config_parse_error: None,
            },
            &[],
        );
        assert!(prompt.contains("Task: produce the next repair plan"));
        assert!(prompt.contains("Target location: remote_openclaw"));
        assert!(prompt.contains("\"planKind\": \"repair\""));
        assert!(prompt.contains("\"configExcerpt\""));
        assert!(prompt.contains("clawpal doctor probe-openclaw"));
        assert!(prompt.contains("openclaw gateway status"));
        assert!(prompt.contains("Output valid JSON only."));
    }

    #[test]
    fn default_remote_doctor_protocol_prefers_agent() {
        assert_eq!(
            default_remote_doctor_protocol(),
            RemoteDoctorProtocol::AgentPlanner
        );
    }

    #[test]
    fn unreadable_config_requires_investigate_plan_kind() {
        let diagnosis = sample_diagnosis(vec![json!({
            "id": "primary.config.unreadable",
            "code": "primary.config.unreadable",
            "severity": "error",
            "message": "Primary configuration could not be read",
            "autoFixable": false,
            "fixHint": "Repair openclaw.json parsing errors and re-run the primary recovery check",
            "source": "primary"
        })]);
        assert_eq!(next_agent_plan_kind(&diagnosis), PlanKind::Investigate);
    }

    #[test]
    fn unreadable_config_switches_to_repair_after_investigation_results_exist() {
        let diagnosis = sample_diagnosis(vec![json!({
            "id": "primary.config.unreadable",
            "code": "primary.config.unreadable",
            "severity": "error",
            "message": "Primary configuration could not be read",
            "autoFixable": false,
            "fixHint": "Repair openclaw.json parsing errors and re-run the primary recovery check",
            "source": "primary"
        })]);
        let previous_results = vec![CommandResult {
            argv: vec!["clawpal".into(), "doctor".into(), "config-read-raw".into()],
            exit_code: Some(0),
            stdout: "{\"raw\":\"{\\n  ddd\\n}\"}".into(),
            stderr: String::new(),
            duration_ms: 1,
            timed_out: false,
        }];
        assert_eq!(
            next_agent_plan_kind_for_round(&diagnosis, &previous_results),
            PlanKind::Repair
        );
    }

    #[test]
    fn non_auto_fixable_warning_only_diagnosis_is_terminal() {
        let diagnosis = sample_diagnosis(vec![json!({
            "id": "rescue.gateway.unhealthy",
            "code": "rescue.gateway.unhealthy",
            "severity": "warn",
            "message": "Rescue gateway is not healthy",
            "autoFixable": false,
            "fixHint": "Inspect rescue gateway logs before using failover",
            "source": "rescue"
        })]);
        assert!(diagnosis_has_only_non_auto_fixable_issues(&diagnosis));
    }

    #[test]
    fn investigate_prompt_requires_read_only_diagnosis_steps() {
        let diagnosis = sample_diagnosis(vec![json!({
            "id": "primary.config.unreadable",
            "code": "primary.config.unreadable",
            "severity": "error",
            "message": "Primary configuration could not be read",
            "autoFixable": false,
            "fixHint": "Repair openclaw.json parsing errors and re-run the primary recovery check",
            "source": "primary"
        })]);
        let prompt = build_agent_plan_prompt(
            PlanKind::Investigate,
            "sess-1",
            1,
            TargetLocation::RemoteOpenclaw,
            "ssh:vm1",
            &diagnosis,
            &build_config_excerpt_context("{\n  ddd\n}"),
            &[],
        );
        assert!(prompt.contains("read-only"));
        assert!(prompt.contains("Do not modify files"));
        assert!(prompt.contains("\"planKind\": \"investigate\""));
        assert!(prompt.contains("configParseError"));
    }

    #[test]
    fn investigate_prompt_discourages_long_running_log_commands() {
        let prompt = build_agent_plan_prompt(
            PlanKind::Investigate,
            "sess-1",
            1,
            TargetLocation::RemoteOpenclaw,
            "ssh:vm1",
            &sample_diagnosis(Vec::new()),
            &empty_config_excerpt_context(),
            &[],
        );
        assert!(prompt.contains("Do not run follow/tail commands"));
        assert!(prompt.contains("bounded"));
        assert!(prompt.contains("Do not use heredocs"));
    }

    #[test]
    fn repair_prompt_discourages_unverified_openclaw_subcommands() {
        let prompt = build_agent_plan_prompt(
            PlanKind::Repair,
            "sess-1",
            2,
            TargetLocation::RemoteOpenclaw,
            "ssh:vm1",
            &sample_diagnosis(Vec::new()),
            &empty_config_excerpt_context(),
            &[],
        );
        assert!(prompt.contains("Do not invent OpenClaw subcommands"));
        assert!(prompt.contains("Do not use `openclaw auth"));
        assert!(prompt.contains("Do not use `openclaw doctor --json`"));
        assert!(!prompt.contains("- `openclaw doctor --json`"));
    }

    #[test]
    fn remote_doctor_agent_id_is_dedicated() {
        assert_eq!(remote_doctor_agent_id(), "clawpal-remote-doctor");
        assert!(!remote_doctor_agent_session_key("sess-1").contains("main"));
        assert!(
            remote_doctor_agent_session_key("sess-1").starts_with("agent:clawpal-remote-doctor:")
        );
    }

    #[test]
    fn ensure_local_remote_doctor_agent_creates_workspace_bootstrap_files() {
        let temp_root = std::env::temp_dir().join(format!(
            "clawpal-remote-doctor-agent-test-{}",
            Uuid::new_v4()
        ));
        let home_dir = temp_root.join("home");
        let clawpal_dir = temp_root.join("clawpal");
        let openclaw_dir = home_dir.join(".openclaw");
        std::fs::create_dir_all(&openclaw_dir).expect("create openclaw dir");
        std::fs::create_dir_all(&clawpal_dir).expect("create clawpal dir");
        std::fs::write(
            openclaw_dir.join("openclaw.json"),
            r#"{
  "gateway": { "port": 18789, "auth": { "token": "gw-test-token" } },
  "agents": {
    "defaults": { "model": "openai/gpt-4o-mini" },
    "list": [{ "id": "main", "workspace": "~/.openclaw/workspaces/main" }]
  }
}
"#,
        )
        .expect("write config");

        set_active_openclaw_home_override(Some(home_dir.to_string_lossy().to_string()))
            .expect("set openclaw override");
        set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
            .expect("set clawpal override");

        let result = ensure_local_remote_doctor_agent_ready();

        set_active_openclaw_home_override(None).expect("clear openclaw override");
        set_active_clawpal_data_override(None).expect("clear clawpal override");

        if let Err(error) = &result {
            let _ = std::fs::remove_dir_all(&temp_root);
            panic!("ensure agent ready: {error}");
        }

        let cfg: Value = serde_json::from_str(
            &std::fs::read_to_string(openclaw_dir.join("openclaw.json")).expect("read config"),
        )
        .expect("parse config");
        let agent = cfg["agents"]["list"]
            .as_array()
            .and_then(|agents| {
                agents.iter().find(|agent| {
                    agent.get("id").and_then(Value::as_str) == Some(remote_doctor_agent_id())
                })
            })
            .expect("dedicated agent entry");
        let workspace = agent["workspace"]
            .as_str()
            .expect("agent workspace")
            .replace("~/", &format!("{}/", home_dir.to_string_lossy()));
        for file_name in ["IDENTITY.md", "USER.md", "BOOTSTRAP.md", "AGENTS.md"] {
            let content = std::fs::read_to_string(std::path::Path::new(&workspace).join(file_name))
                .unwrap_or_else(|error| panic!("read {file_name}: {error}"));
            assert!(
                !content.trim().is_empty(),
                "{file_name} should not be empty"
            );
        }

        let _ = std::fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn only_agent_planner_protocol_requires_bridge() {
        assert!(protocol_requires_bridge(RemoteDoctorProtocol::AgentPlanner));
        assert!(!protocol_requires_bridge(
            RemoteDoctorProtocol::ClawpalServer
        ));
        assert!(!protocol_requires_bridge(
            RemoteDoctorProtocol::LegacyDoctor
        ));
    }

    #[test]
    fn clawpal_server_protocol_skips_local_rescue_preflight() {
        assert!(!protocol_runs_rescue_preflight(
            RemoteDoctorProtocol::ClawpalServer
        ));
        assert!(!protocol_runs_rescue_preflight(
            RemoteDoctorProtocol::AgentPlanner
        ));
    }

    #[test]
    fn remote_target_host_id_candidates_include_exact_and_stripped_ids() {
        assert_eq!(
            remote_target_host_id_candidates("ssh:15-235-214-81"),
            vec!["ssh:15-235-214-81".to_string(), "15-235-214-81".to_string()]
        );
        assert_eq!(
            remote_target_host_id_candidates("e2e-remote-doctor"),
            vec!["e2e-remote-doctor".to_string()]
        );
    }

    #[test]
    fn primary_remote_target_host_id_prefers_exact_instance_id() {
        assert_eq!(
            primary_remote_target_host_id("ssh:15-235-214-81").unwrap(),
            "ssh:15-235-214-81"
        );
    }

    #[test]
    fn parse_invoke_argv_supports_command_string_payloads() {
        let argv = parse_invoke_argv(
            "clawpal",
            &json!({
                "command": "doctor config-read models.providers.openai"
            }),
        )
        .expect("parse invoke argv");
        assert_eq!(
            argv,
            vec![
                "clawpal",
                "doctor",
                "config-read",
                "models.providers.openai"
            ]
        );
    }

    #[test]
    fn plan_commands_treat_clawpal_as_internal_tool() {
        assert!(plan_command_uses_internal_clawpal_tool(&[
            "clawpal".to_string(),
            "doctor".to_string(),
            "config-read".to_string(),
        ]));
        assert!(!plan_command_uses_internal_clawpal_tool(&[
            "openclaw".to_string(),
            "doctor".to_string(),
        ]));
    }

    #[test]
    fn unsupported_openclaw_subcommand_is_rejected_early() {
        let error = validate_plan_command_argv(&[
            "openclaw".to_string(),
            "auth".to_string(),
            "list".to_string(),
        ])
        .unwrap_err();
        assert!(error.contains("Unsupported openclaw plan command"));
        assert!(error.contains("openclaw auth list"));
    }

    #[test]
    fn openclaw_doctor_json_is_rejected_early() {
        let error = validate_plan_command_argv(&[
            "openclaw".to_string(),
            "doctor".to_string(),
            "--json".to_string(),
        ])
        .unwrap_err();
        assert!(error.contains("Unsupported openclaw plan command"));
        assert!(error.contains("openclaw doctor --json"));
    }

    #[test]
    fn multiline_clawpal_exec_is_rejected_early() {
        let error = validate_plan_command_argv(&[
            "clawpal".to_string(),
            "doctor".to_string(),
            "exec".to_string(),
            "--tool".to_string(),
            "python3".to_string(),
            "--args".to_string(),
            "- <<'PY'\nprint('hi')\nPY".to_string(),
        ])
        .unwrap_err();
        assert!(error.contains("Unsupported clawpal doctor exec args"));
        assert!(error.contains("heredocs"));
    }

    #[test]
    fn plan_command_failure_message_mentions_command_and_error() {
        let error = plan_command_failure_message(
            PlanKind::Investigate,
            2,
            &[
                "openclaw".to_string(),
                "gateway".to_string(),
                "logs".to_string(),
            ],
            "ssh command failed: russh exec timed out after 25s",
        );
        assert!(error.contains("Investigate command failed in round 2"));
        assert!(error.contains("openclaw gateway logs"));
        assert!(error.contains("timed out after 25s"));
    }

    fn sample_diagnosis(issues: Vec<Value>) -> RescuePrimaryDiagnosisResult {
        serde_json::from_value(json!({
            "status": if issues.is_empty() { "healthy" } else { "broken" },
            "checkedAt": "2026-03-18T00:00:00Z",
            "targetProfile": "primary",
            "rescueProfile": "rescue",
            "rescueConfigured": true,
            "rescuePort": 18789,
            "summary": {
                "status": if issues.is_empty() { "healthy" } else { "broken" },
                "headline": if issues.is_empty() { "Healthy" } else { "Broken" },
                "recommendedAction": if issues.is_empty() { "No action needed" } else { "Repair issues" },
                "fixableIssueCount": issues.len(),
                "selectedFixIssueIds": issues.iter().filter_map(|issue| issue.get("id").and_then(Value::as_str)).collect::<Vec<_>>(),
                "rootCauseHypotheses": [],
                "fixSteps": [],
                "confidence": 0.8,
                "citations": [],
                "versionAwareness": null
            },
            "sections": [],
            "checks": [],
            "issues": issues
        }))
        .expect("sample diagnosis")
    }

    #[test]
    fn diagnosis_issue_summaries_capture_code_severity_and_message() {
        let diagnosis = sample_diagnosis(vec![
            json!({
                "id": "gateway.unhealthy",
                "code": "gateway.unhealthy",
                "severity": "high",
                "message": "Gateway is unhealthy",
                "autoFixable": true,
                "fixHint": "Restart gateway",
                "source": "gateway"
            }),
            json!({
                "id": "providers.base_url",
                "code": "invalid.base_url",
                "severity": "medium",
                "message": "Provider base URL is invalid",
                "autoFixable": true,
                "fixHint": "Reset baseUrl",
                "source": "config"
            }),
        ]);

        let summary = diagnosis_issue_summaries(&diagnosis);
        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0]["code"], "gateway.unhealthy");
        assert_eq!(summary[0]["severity"], "high");
        assert_eq!(summary[0]["title"], "Gateway is unhealthy");
        assert_eq!(summary[0]["target"], "gateway");
        assert_eq!(summary[1]["code"], "invalid.base_url");
    }

    #[test]
    fn repeated_rediagnose_only_rounds_are_detected_as_stalled() {
        let diagnosis = sample_diagnosis(vec![json!({
            "id": "providers.base_url",
            "code": "invalid.base_url",
            "severity": "medium",
            "message": "Provider base URL is invalid",
            "autoFixable": true,
            "fixHint": "Reset baseUrl",
            "source": "config"
        })]);
        let step_types = vec!["doctorRediagnose".to_string()];

        assert!(!repair_loops_repair_plan_stalled(
            &[
                RepairRoundObservation::new(1, &step_types, &diagnosis),
                RepairRoundObservation::new(2, &step_types, &diagnosis),
            ],
            3,
        ));
        assert!(repair_loops_repair_plan_stalled(
            &[
                RepairRoundObservation::new(1, &step_types, &diagnosis),
                RepairRoundObservation::new(2, &step_types, &diagnosis),
                RepairRoundObservation::new(3, &step_types, &diagnosis),
            ],
            3,
        ));
    }

    #[test]
    fn round_limit_error_message_includes_latest_issues_and_step_types() {
        let diagnosis = sample_diagnosis(vec![json!({
            "id": "providers.base_url",
            "code": "invalid.base_url",
            "severity": "medium",
            "message": "Provider base URL is invalid",
            "autoFixable": true,
            "fixHint": "Reset baseUrl",
            "source": "config"
        })]);
        let error = repair_loops_round_limit_error_message(
            &diagnosis,
            &["doctorRediagnose".to_string()],
        );
        assert!(error.contains("invalid.base_url"));
        assert!(error.contains("doctorRediagnose"));
        assert!(error.contains("Provider base URL is invalid"));
    }

    #[test]
    fn unreadable_config_context_uses_raw_excerpt_and_parse_error() {
        let context = build_config_excerpt_context("{\n  ddd\n}");
        assert!(context.config_excerpt.is_null());
        assert!(context
            .config_excerpt_raw
            .as_deref()
            .unwrap_or_default()
            .contains("ddd"));
        assert!(context
            .config_parse_error
            .as_deref()
            .unwrap_or_default()
            .contains("key must be a string"));
    }

    #[test]
    fn unreadable_config_context_summary_marks_excerpt_missing() {
        let context = build_config_excerpt_context("{\n  ddd\n}");
        let summary = config_excerpt_log_summary(&context);
        assert_eq!(summary["configExcerptPresent"], json!(false));
        assert_eq!(summary["configExcerptRawPresent"], json!(true));
        assert!(summary["configParseError"]
            .as_str()
            .unwrap_or_default()
            .contains("key must be a string"));
    }

    #[test]
    fn config_read_response_returns_raw_context_for_unreadable_json() {
        let value = config_read_response("{\n  ddd\n}", None).expect("config read response");
        assert!(value["value"].is_null());
        assert!(value["raw"].as_str().unwrap_or_default().contains("ddd"));
        assert!(value["parseError"]
            .as_str()
            .unwrap_or_default()
            .contains("key must be a string"));
    }

    #[test]
    fn decode_base64_config_payload_reads_utf8_text() {
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode("{\"ok\":true}");
        let decoded = decode_base64_config_payload(&encoded).expect("decode payload");
        assert_eq!(decoded, "{\"ok\":true}");
    }

    #[test]
    fn diagnosis_missing_rescue_profile_is_detected() {
        let diagnosis = sample_diagnosis(vec![json!({
            "id": "rescue.profile.missing",
            "code": "rescue.profile.missing",
            "severity": "error",
            "message": "Rescue profile \"rescue\" is not configured",
            "autoFixable": false,
            "fixHint": "Activate Rescue Bot first",
            "source": "rescue"
        })]);
        assert!(diagnosis_missing_rescue_profile(&diagnosis));
    }

    #[test]
    fn diagnosis_unhealthy_rescue_gateway_is_detected() {
        let diagnosis = sample_diagnosis(vec![json!({
            "id": "rescue.gateway.unhealthy",
            "code": "rescue.gateway.unhealthy",
            "severity": "warn",
            "message": "Rescue gateway is not healthy",
            "autoFixable": false,
            "fixHint": "Inspect rescue gateway logs before using failover",
            "source": "rescue"
        })]);
        assert!(diagnosis_unhealthy_rescue_gateway(&diagnosis));
    }

    #[test]
    fn rescue_setup_command_result_reports_activation() {
        let result = rescue_setup_command_result("activate", "rescue", true, true, "active");
        assert_eq!(result.argv, vec!["manage_rescue_bot", "activate", "rescue"]);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("configured=true"));
        assert!(result.stdout.contains("active=true"));
    }

    #[test]
    fn rescue_setup_activation_error_mentions_runtime_state() {
        let error = rescue_activation_error_message(
            "rescue",
            false,
            "configured_inactive",
            &[
                "manage_rescue_bot status rescue".to_string(),
                "openclaw --profile rescue gateway status".to_string(),
            ],
        );
        assert!(error.contains("rescue"));
        assert!(error.contains("configured_inactive"));
        assert!(error.contains("did not become active"));
        assert!(error.contains("manage_rescue_bot status rescue"));
        assert!(error.contains("openclaw --profile rescue gateway status"));
    }

    #[test]
    fn rescue_activation_diagnostic_commands_include_status_and_gateway_checks() {
        let commands = rescue_activation_diagnostic_commands("rescue");
        let rendered = commands
            .iter()
            .map(|command| command.join(" "))
            .collect::<Vec<_>>();
        assert!(rendered.contains(&"manage_rescue_bot status rescue".to_string()));
        assert!(rendered.contains(&"openclaw --profile rescue gateway status".to_string()));
        assert!(rendered
            .contains(&"openclaw --profile rescue config get gateway.port --json".to_string()));
    }

    const E2E_CONTAINER_NAME: &str = "clawpal-e2e-remote-doctor";
    const E2E_SSH_PORT: u16 = 2399;
    const E2E_ROOT_PASSWORD: &str = "clawpal-remote-doctor-pass";
    const E2E_DOCKERFILE: &str = r#"
FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y openssh-server && rm -rf /var/lib/apt/lists/* && mkdir /var/run/sshd
RUN echo "root:ROOTPASS" | chpasswd && \
    sed -i 's/#PermitRootLogin.*/PermitRootLogin yes/' /etc/ssh/sshd_config && \
    sed -i 's/PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config && \
    echo "PasswordAuthentication yes" >> /etc/ssh/sshd_config
RUN mkdir -p /root/.openclaw
RUN cat > /root/.openclaw/openclaw.json <<'EOF'
{
  "gateway": { "port": 18789, "auth": { "token": "gw-test-token" } },
  "auth": {
    "profiles": {
      "openai-default": {
        "provider": "openai",
        "apiKey": "sk-test"
      }
    }
  },
  "models": {
    "providers": {
      "openai": {
        "baseUrl": "http://127.0.0.1:9/v1",
        "models": [{ "id": "gpt-4o-mini", "name": "gpt-4o-mini" }]
      }
    }
  },
  "agents": {
    "defaults": { "model": "openai/gpt-4o-mini" },
    "list": [ { "id": "main", "model": "anthropic/claude-sonnet-4-20250514" } ]
  },
  "channels": {
    "discord": {
      "guilds": {
        "guild-1": {
          "channels": {
            "general": { "model": "openai/gpt-4o-mini" }
          }
        }
      }
    }
  }
}
EOF
RUN cat > /usr/local/bin/openclaw <<'EOF' && chmod +x /usr/local/bin/openclaw
#!/bin/sh
STATE_DIR="${OPENCLAW_STATE_DIR:-${OPENCLAW_HOME:-$HOME/.openclaw}}"
CONFIG_PATH="$STATE_DIR/openclaw.json"
PROFILE="primary"
if [ "$1" = "--profile" ]; then
  PROFILE="$2"
  shift 2
fi
case "$1" in
  --version)
    echo "openclaw 2026.3.2-test"
    ;;
  doctor)
    if grep -q '127.0.0.1:9/v1' "$CONFIG_PATH"; then
      echo '{"ok":false,"score":40,"issues":[{"id":"primary.models.base_url","code":"invalid.base_url","severity":"error","message":"provider baseUrl points to test blackhole","autoFixable":true,"fixHint":"Remove the bad baseUrl override"}]}'
    else
      echo '{"ok":true,"score":100,"issues":[],"checks":[{"id":"test","status":"ok"}]}'
    fi
    ;;
  agents)
    if [ "$2" = "list" ] && [ "$3" = "--json" ]; then
      echo '[{"id":"main"}]'
    else
      echo "unsupported openclaw agents command" >&2
      exit 1
    fi
    ;;
  models)
    if [ "$2" = "list" ] && [ "$3" = "--all" ] && [ "$4" = "--json" ] && [ "$5" = "--no-color" ]; then
      echo '{"models":[{"key":"openai/gpt-4o-mini","provider":"openai","id":"gpt-4o-mini","name":"gpt-4o-mini","baseUrl":"https://api.openai.com/v1"}],"providers":{"openai":{"baseUrl":"https://api.openai.com/v1"}}}'
    else
      echo "unsupported openclaw models command" >&2
      exit 1
    fi
    ;;
  config)
    if [ "$2" = "get" ] && [ "$3" = "gateway.port" ] && [ "$4" = "--json" ]; then
      if [ "$PROFILE" = "rescue" ]; then
        echo '19789'
      else
        echo '18789'
      fi
    else
      echo "unsupported openclaw config command: $*" >&2
      exit 1
    fi
    ;;
  gateway)
    case "$2" in
      status)
        if [ "$PROFILE" = "rescue" ] && [ "${OPENCLAW_RESCUE_GATEWAY_ACTIVE:-1}" != "1" ]; then
          echo '{"running":false,"healthy":false,"gateway":{"running":false},"health":{"ok":false}}'
        else
          echo '{"running":true,"healthy":true,"gateway":{"running":true},"health":{"ok":true}}'
        fi
        ;;
      restart|start|stop)
        echo '{"ok":true}'
        ;;
      *)
        echo "unsupported openclaw gateway command: $*" >&2
        exit 1
        ;;
    esac
    ;;
  *)
    echo "unsupported openclaw command: $*" >&2
    exit 1
    ;;
esac
EOF
EXPOSE 22
CMD ["/usr/sbin/sshd", "-D"]
"#;

    fn should_run_docker_e2e() -> bool {
        std::env::var("CLAWPAL_RUN_REMOTE_DOCTOR_E2E")
            .ok()
            .as_deref()
            == Some("1")
    }

    fn live_gateway_url() -> Option<String> {
        std::env::var("CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    fn live_gateway_token() -> Option<String> {
        std::env::var("CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_TOKEN")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    fn live_gateway_instance_id() -> String {
        std::env::var("CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_INSTANCE_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "local".to_string())
    }

    fn live_gateway_target_location() -> TargetLocation {
        match std::env::var("CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_TARGET_LOCATION")
            .ok()
            .as_deref()
        {
            Some("remote_openclaw") => TargetLocation::RemoteOpenclaw,
            _ => TargetLocation::LocalOpenclaw,
        }
    }

    fn live_gateway_protocol() -> String {
        std::env::var("CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_PROTOCOL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "clawpal_server".to_string())
    }

    fn docker_available() -> bool {
        Command::new("docker")
            .args(["info"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn cleanup_e2e_container() {
        let _ = Command::new("docker")
            .args(["rm", "-f", E2E_CONTAINER_NAME])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let _ = Command::new("docker")
            .args(["rmi", "-f", &format!("{E2E_CONTAINER_NAME}:latest")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    fn build_e2e_image() -> Result<(), String> {
        let dockerfile = E2E_DOCKERFILE.replace("ROOTPASS", E2E_ROOT_PASSWORD);
        let output = Command::new("docker")
            .args([
                "build",
                "-t",
                &format!("{E2E_CONTAINER_NAME}:latest"),
                "-f",
                "-",
                ".",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .current_dir(std::env::temp_dir())
            .spawn()
            .and_then(|mut child| {
                if let Some(ref mut stdin) = child.stdin {
                    stdin.write_all(dockerfile.as_bytes())?;
                }
                child.wait_with_output()
            })
            .map_err(|error| format!("docker build failed: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        Ok(())
    }

    fn start_e2e_container() -> Result<(), String> {
        start_e2e_container_with_env(&[])
    }

    fn start_e2e_container_with_env(env: &[(&str, &str)]) -> Result<(), String> {
        let mut args = vec![
            "run".to_string(),
            "-d".to_string(),
            "--name".to_string(),
            E2E_CONTAINER_NAME.to_string(),
        ];
        for (key, value) in env {
            args.push("-e".to_string());
            args.push(format!("{key}={value}"));
        }
        args.extend([
            "-p".to_string(),
            format!("{E2E_SSH_PORT}:22"),
            format!("{E2E_CONTAINER_NAME}:latest"),
        ]);
        let output = Command::new("docker")
            .args(&args)
            .output()
            .map_err(|error| format!("docker run failed: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        Ok(())
    }

    fn wait_for_ssh(timeout_secs: u64) -> Result<(), String> {
        let start = Instant::now();
        while start.elapsed().as_secs() < timeout_secs {
            if TcpStream::connect(format!("127.0.0.1:{E2E_SSH_PORT}")).is_ok() {
                std::thread::sleep(std::time::Duration::from_millis(500));
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        Err("timeout waiting for ssh".into())
    }

    fn e2e_host_config() -> SshHostConfig {
        SshHostConfig {
            id: "e2e-remote-doctor".into(),
            label: "E2E Remote Doctor".into(),
            host: "127.0.0.1".into(),
            port: E2E_SSH_PORT,
            username: "root".into(),
            auth_method: "password".into(),
            key_path: None,
            password: Some(E2E_ROOT_PASSWORD.into()),
            passphrase: None,
        }
    }

    #[tokio::test]
    async fn remote_doctor_docker_e2e_loop_completes() {
        if !should_run_docker_e2e() {
            eprintln!("skip: set CLAWPAL_RUN_REMOTE_DOCTOR_E2E=1 to enable");
            return;
        }
        if !docker_available() {
            eprintln!("skip: docker not available");
            return;
        }

        cleanup_e2e_container();
        build_e2e_image().expect("docker build");
        start_e2e_container().expect("docker run");
        struct Cleanup;
        impl Drop for Cleanup {
            fn drop(&mut self) {
                cleanup_e2e_container();
            }
        }
        let _cleanup = Cleanup;
        wait_for_ssh(30).expect("ssh should become available");

        let temp_root =
            std::env::temp_dir().join(format!("clawpal-remote-doctor-e2e-{}", Uuid::new_v4()));
        let clawpal_dir = temp_root.join(".clawpal");
        create_dir_all(&clawpal_dir).expect("create clawpal dir");
        set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
            .expect("set clawpal data");
        set_active_openclaw_home_override(None).expect("clear openclaw home override");

        let pool = SshConnectionPool::new();
        let cfg = e2e_host_config();
        pool.connect(&cfg).await.expect("ssh connect");

        let session_id = Uuid::new_v4().to_string();
        let marker = "/tmp/clawpal-remote-doctor-fixed";
        let result = repair_loops_run_remote_doctor_repair_loop(
            Option::<&AppHandle<tauri::test::MockRuntime>>::None,
            &pool,
            &session_id,
            &format!("ssh:{}", cfg.id),
            TargetLocation::RemoteOpenclaw,
            |kind, round, previous_results| async move {
                match (kind, round) {
                    (PlanKind::Detect, 1) => Ok(PlanResponse {
                        plan_id: "detect-1".into(),
                        plan_kind: PlanKind::Detect,
                        summary: "Initial detect".into(),
                        commands: vec![PlanCommand {
                            argv: vec!["openclaw".into(), "--version".into()],
                            timeout_sec: Some(10),
                            purpose: Some("collect version".into()),
                            continue_on_failure: Some(false),
                        }],
                        healthy: false,
                        done: false,
                        success: false,
                    }),
                    (PlanKind::Repair, 1) => {
                        assert_eq!(previous_results.len(), 1);
                        Ok(PlanResponse {
                            plan_id: "repair-1".into(),
                            plan_kind: PlanKind::Repair,
                            summary: "Write marker".into(),
                            commands: vec![PlanCommand {
                                argv: vec![
                                    "sh".into(),
                                    "-lc".into(),
                                    format!("printf 'fixed' > {marker}"),
                                ],
                                timeout_sec: Some(10),
                                purpose: Some("mark repaired".into()),
                                continue_on_failure: Some(false),
                            }],
                            healthy: false,
                            done: false,
                            success: false,
                        })
                    }
                    (PlanKind::Detect, 2) => {
                        assert_eq!(previous_results.len(), 1);
                        assert_eq!(
                            previous_results[0].stdout.trim(),
                            "",
                            "repair command should not print to stdout"
                        );
                        Ok(PlanResponse {
                            plan_id: "detect-2".into(),
                            plan_kind: PlanKind::Detect,
                            summary: "Marker exists".into(),
                            commands: Vec::new(),
                            healthy: true,
                            done: true,
                            success: true,
                        })
                    }
                    _ => Err(format!(
                        "unexpected planner request: {:?} round {}",
                        kind, round
                    )),
                }
            },
        )
        .await
        .expect("remote doctor loop should complete");

        assert_eq!(result.status, "completed");
        assert!(result.latest_diagnosis_healthy);
        assert_eq!(result.round, 2);

        let marker_result = pool
            .exec(&cfg.id, &format!("test -f {marker}"))
            .await
            .expect("marker check");
        assert_eq!(marker_result.exit_code, 0);

        let log_path = clawpal_dir
            .join("doctor")
            .join("remote")
            .join(format!("{session_id}.jsonl"));
        let log_text = std::fs::read_to_string(&log_path).expect("read remote doctor log");
        assert!(log_text.contains("\"planKind\":\"detect\""));
        assert!(log_text.contains("\"planKind\":\"repair\""));
        let _ = std::fs::remove_dir_all(temp_root);
        set_active_clawpal_data_override(None).expect("clear clawpal data");
    }

    #[tokio::test]
    async fn remote_doctor_docker_e2e_rescue_activation_fails_when_gateway_stays_inactive() {
        if !should_run_docker_e2e() {
            eprintln!("skip: set CLAWPAL_RUN_REMOTE_DOCTOR_E2E=1 to enable");
            return;
        }
        if !docker_available() {
            eprintln!("skip: docker not available");
            return;
        }

        cleanup_e2e_container();
        build_e2e_image().expect("docker build");
        start_e2e_container_with_env(&[("OPENCLAW_RESCUE_GATEWAY_ACTIVE", "0")])
            .expect("docker run");
        struct Cleanup;
        impl Drop for Cleanup {
            fn drop(&mut self) {
                cleanup_e2e_container();
            }
        }
        let _cleanup = Cleanup;
        wait_for_ssh(30).expect("ssh should become available");

        let app = mock_app();
        let app_handle = app.handle().clone();
        app_handle.manage(SshConnectionPool::new());
        let pool = app_handle.state::<SshConnectionPool>();
        let cfg = e2e_host_config();
        pool.connect(&cfg).await.expect("ssh connect");

        let error = ensure_rescue_profile_ready(
            &app_handle,
            TargetLocation::RemoteOpenclaw,
            &format!("ssh:{}", cfg.id),
        )
        .await
        .expect_err("rescue activation should fail when gateway remains inactive");

        assert!(error.message.contains("did not become active"));
        assert!(error.message.contains("configured_inactive"));
        assert!(error
            .diagnostics
            .iter()
            .any(|result| result.argv.join(" ") == "manage_rescue_bot status rescue"));
    }

    #[tokio::test]
    async fn remote_doctor_live_gateway_uses_configured_url_and_token() {
        let Some(url) = live_gateway_url() else {
            eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_URL to enable");
            return;
        };
        let Some(token) = live_gateway_token() else {
            eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_TOKEN to enable");
            return;
        };

        let app = mock_app();
        let app_handle = app.handle().clone();
        app_handle.manage(SshConnectionPool::new());
        let temp_root =
            std::env::temp_dir().join(format!("clawpal-remote-doctor-live-{}", Uuid::new_v4()));
        let clawpal_dir = temp_root.join(".clawpal");
        create_dir_all(&clawpal_dir).expect("create clawpal dir");
        set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
            .expect("set clawpal data");

        std::fs::write(
            clawpal_dir.join("app-preferences.json"),
            serde_json::to_string(&json!({
                "remoteDoctorGatewayUrl": url,
                "remoteDoctorGatewayAuthToken": token,
            }))
            .expect("serialize prefs"),
        )
        .expect("write app preferences");

        let gateway = remote_doctor_gateway_config().expect("gateway config");
        assert_eq!(gateway.url, url);
        assert_eq!(gateway.auth_token_override.as_deref(), Some(token.as_str()));

        let creds = remote_doctor_gateway_credentials(gateway.auth_token_override.as_deref())
            .expect("gateway credentials");
        assert!(creds.is_some(), "expected token override credentials");

        let client = NodeClient::new();
        client
            .connect(&gateway.url, app.handle().clone(), creds)
            .await
            .expect("connect live remote doctor gateway");
        assert!(client.is_connected().await);
        match live_gateway_protocol().as_str() {
            "clawpal_server" => {
                let response = client
                    .send_request(
                        "remote_repair_plan.request",
                        json!({
                            "requestId": format!("live-e2e-{}", Uuid::new_v4()),
                            "targetId": live_gateway_instance_id(),
                            "context": {
                                "configExcerpt": {
                                    "models": {
                                        "providers": {
                                            "openai-codex": {
                                                "baseUrl": "http://127.0.0.1:9/v1"
                                            }
                                        }
                                    }
                                }
                            }
                        }),
                    )
                    .await
                    .expect("request clawpal-server remote repair plan");
                let plan_id = response
                    .get("planId")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                assert!(
                    !plan_id.trim().is_empty(),
                    "clawpal-server response should include a plan id"
                );
                let steps = response
                    .get("steps")
                    .and_then(|value| value.as_array())
                    .cloned()
                    .unwrap_or_default();
                assert!(
                    !steps.is_empty(),
                    "clawpal-server response should include repair steps"
                );
            }
            _ => {
                let detect_plan = request_plan(
                    &client,
                    &detect_method_name(),
                    PlanKind::Detect,
                    &format!("live-e2e-{}", Uuid::new_v4()),
                    1,
                    live_gateway_target_location(),
                    &live_gateway_instance_id(),
                    &[],
                )
                .await
                .expect("request live detection plan");
                assert!(
                    !detect_plan.plan_id.trim().is_empty(),
                    "live detection plan should include a plan id"
                );
            }
        }
        client.disconnect().await.expect("disconnect");

        set_active_clawpal_data_override(None).expect("clear clawpal data");
        let _ = std::fs::remove_dir_all(temp_root);
    }

    #[tokio::test]
    async fn remote_doctor_live_gateway_full_repair_loop_completes() {
        let Some(url) = live_gateway_url() else {
            eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_URL to enable");
            return;
        };
        let Some(token) = live_gateway_token() else {
            eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_TOKEN to enable");
            return;
        };
        if !docker_available() {
            eprintln!("skip: docker not available");
            return;
        }

        cleanup_e2e_container();
        build_e2e_image().expect("docker build");
        start_e2e_container().expect("docker run");
        struct Cleanup;
        impl Drop for Cleanup {
            fn drop(&mut self) {
                cleanup_e2e_container();
            }
        }
        let _cleanup = Cleanup;
        wait_for_ssh(30).expect("ssh should become available");

        let app = mock_app();
        let app_handle = app.handle().clone();
        app_handle.manage(SshConnectionPool::new());
        let temp_root = std::env::temp_dir().join(format!(
            "clawpal-remote-doctor-live-loop-{}",
            Uuid::new_v4()
        ));
        let clawpal_dir = temp_root.join(".clawpal");
        create_dir_all(&clawpal_dir).expect("create clawpal dir");
        set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
            .expect("set clawpal data");
        set_active_openclaw_home_override(None).expect("clear openclaw home override");

        std::fs::write(
            clawpal_dir.join("app-preferences.json"),
            serde_json::to_string(&json!({
                "remoteDoctorGatewayUrl": url,
                "remoteDoctorGatewayAuthToken": token,
            }))
            .expect("serialize prefs"),
        )
        .expect("write app preferences");

        let cfg = e2e_host_config();
        let pool = app_handle.state::<SshConnectionPool>();
        pool.connect(&cfg).await.expect("ssh connect");

        let gateway = remote_doctor_gateway_config().expect("gateway config");
        let creds = remote_doctor_gateway_credentials(gateway.auth_token_override.as_deref())
            .expect("gateway credentials");
        let client = NodeClient::new();
        client
            .connect(&gateway.url, app_handle.clone(), creds)
            .await
            .expect("connect live remote doctor gateway");

        let session_id = Uuid::new_v4().to_string();
        let result = repair_loops_run_clawpal_server_repair_loop(
            &app_handle,
            &client,
            &session_id,
            &format!("ssh:{}", cfg.id),
            TargetLocation::RemoteOpenclaw,
        )
        .await
        .expect("full live remote doctor repair loop should complete");

        assert_eq!(result.status, "completed");
        assert!(result.latest_diagnosis_healthy);

        client.disconnect().await.expect("disconnect");
        set_active_clawpal_data_override(None).expect("clear clawpal data");
        let _ = std::fs::remove_dir_all(temp_root);
    }

    #[tokio::test]
    async fn remote_doctor_live_start_command_remote_target_completes_without_bridge_pairing() {
        let Some(url) = live_gateway_url() else {
            eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_URL to enable");
            return;
        };
        let Some(token) = live_gateway_token() else {
            eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_TOKEN to enable");
            return;
        };
        if !docker_available() {
            eprintln!("skip: docker not available");
            return;
        }

        cleanup_e2e_container();
        build_e2e_image().expect("docker build");
        start_e2e_container().expect("docker run");
        struct Cleanup;
        impl Drop for Cleanup {
            fn drop(&mut self) {
                cleanup_e2e_container();
            }
        }
        let _cleanup = Cleanup;
        wait_for_ssh(30).expect("ssh should become available");

        let app = mock_app();
        let app_handle = app.handle().clone();
        app_handle.manage(SshConnectionPool::new());
        let temp_root = std::env::temp_dir().join(format!(
            "clawpal-remote-doctor-live-start-{}",
            Uuid::new_v4()
        ));
        let clawpal_dir = temp_root.join(".clawpal");
        create_dir_all(&clawpal_dir).expect("create clawpal dir");
        set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
            .expect("set clawpal data");
        set_active_openclaw_home_override(None).expect("clear openclaw home override");

        std::fs::write(
            clawpal_dir.join("app-preferences.json"),
            serde_json::to_string(&json!({
                "remoteDoctorGatewayUrl": url,
                "remoteDoctorGatewayAuthToken": token,
            }))
            .expect("serialize prefs"),
        )
        .expect("write app preferences");

        let cfg = crate::commands::ssh::upsert_ssh_host(e2e_host_config()).expect("save ssh host");
        let pool = app_handle.state::<SshConnectionPool>();

        let result = repair_loops_start_remote_doctor_repair_impl(
            app_handle.clone(),
            &pool,
            format!("ssh:{}", cfg.id),
            "remote_openclaw".to_string(),
        )
        .await
        .expect("start command should complete remote repair");

        assert_eq!(result.status, "completed");
        assert!(result.latest_diagnosis_healthy);

        let log_path = clawpal_dir
            .join("doctor")
            .join("remote")
            .join(format!("{}.jsonl", result.session_id));
        let log_text = std::fs::read_to_string(&log_path).expect("read remote doctor session log");
        assert!(
            !log_text.contains("\"event\":\"bridge_connect_failed\""),
            "clawpal_server path should not attempt bridge pairing: {log_text}"
        );

        set_active_clawpal_data_override(None).expect("clear clawpal data");
        let _ = std::fs::remove_dir_all(temp_root);
    }

    #[tokio::test]
    async fn remote_doctor_live_gateway_repairs_unreadable_remote_config() {
        let Some(url) = live_gateway_url() else {
            eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_URL to enable");
            return;
        };
        let Some(token) = live_gateway_token() else {
            eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_TOKEN to enable");
            return;
        };
        if !docker_available() {
            eprintln!("skip: docker not available");
            return;
        }

        cleanup_e2e_container();
        build_e2e_image().expect("docker build");
        start_e2e_container().expect("docker run");
        struct Cleanup;
        impl Drop for Cleanup {
            fn drop(&mut self) {
                cleanup_e2e_container();
            }
        }
        let _cleanup = Cleanup;
        wait_for_ssh(30).expect("ssh should become available");

        let app = mock_app();
        let app_handle = app.handle().clone();
        app_handle.manage(SshConnectionPool::new());
        let temp_root = std::env::temp_dir().join(format!(
            "clawpal-remote-doctor-live-raw-config-{}",
            Uuid::new_v4()
        ));
        let clawpal_dir = temp_root.join(".clawpal");
        create_dir_all(&clawpal_dir).expect("create clawpal dir");
        set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
            .expect("set clawpal data");
        set_active_openclaw_home_override(None).expect("clear openclaw home override");

        std::fs::write(
            clawpal_dir.join("app-preferences.json"),
            serde_json::to_string(&json!({
                "remoteDoctorGatewayUrl": url,
                "remoteDoctorGatewayAuthToken": token,
            }))
            .expect("serialize prefs"),
        )
        .expect("write app preferences");

        let cfg = crate::commands::ssh::upsert_ssh_host(e2e_host_config()).expect("save ssh host");
        let pool = app_handle.state::<SshConnectionPool>();
        pool.connect(&cfg).await.expect("ssh connect");
        pool.exec_login(
            &cfg.id,
            "cat > ~/.openclaw/openclaw.json <<'EOF'\n{\n  ddd\n}\nEOF",
        )
        .await
        .expect("corrupt remote config");

        let result = repair_loops_start_remote_doctor_repair_impl(
            app_handle.clone(),
            &pool,
            cfg.id.clone(),
            "remote_openclaw".to_string(),
        )
        .await
        .expect("start command should repair unreadable config");

        assert_eq!(result.status, "completed");
        assert!(result.latest_diagnosis_healthy);

        let repaired = pool
            .exec_login(&cfg.id, "python3 - <<'PY'\nimport json, pathlib\njson.load(open(pathlib.Path.home()/'.openclaw'/'openclaw.json'))\nprint('ok')\nPY")
            .await
            .expect("read repaired config");
        assert_eq!(
            repaired.exit_code, 0,
            "repaired config should be valid JSON: {}",
            repaired.stderr
        );
        assert_eq!(repaired.stdout.trim(), "ok");

        set_active_clawpal_data_override(None).expect("clear clawpal data");
        let _ = std::fs::remove_dir_all(temp_root);
    }
}
