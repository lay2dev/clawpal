use std::fs::create_dir_all;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use base64::Engine;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Runtime, State};
use uuid::Uuid;

use super::config::{
    append_diagnosis_log, build_config_excerpt_context,
    build_gateway_credentials as remote_doctor_gateway_credentials,
    config_excerpt_log_summary, diagnosis_context,
    diagnosis_has_only_non_auto_fixable_issues, diagnosis_is_healthy,
    diagnosis_missing_rescue_profile, diagnosis_unhealthy_rescue_gateway,
    empty_config_excerpt_context, empty_diagnosis,
    load_gateway_config as remote_doctor_gateway_config, primary_remote_target_host_id,
    read_target_config, read_target_config_raw, remote_target_host_id_candidates,
    restart_target_gateway, run_rescue_diagnosis, write_target_config,
    write_target_config_raw, RemoteDoctorGatewayConfig,
};
use super::agent::{
    build_agent_plan_prompt, configured_remote_doctor_protocol, default_remote_doctor_protocol,
    detect_method_name, ensure_agent_workspace_ready as ensure_local_remote_doctor_agent_ready,
    gateway_url_is_local, next_agent_plan_kind, next_agent_plan_kind_for_round,
    protocol_requires_bridge, protocol_runs_rescue_preflight, remote_doctor_agent_id,
    remote_doctor_agent_session_key, repair_method_name,
};
use super::session::{
    append_session_log as append_remote_doctor_log,
    emit_session_progress as emit_progress, result_for_completion,
    result_for_completion_with_warnings,
};
use super::types::{
    diagnosis_issue_summaries, parse_target_location, ClawpalServerPlanResponse,
    ClawpalServerPlanStep, CommandResult, ConfigExcerptContext, PlanCommand, PlanKind,
    PlanResponse, RemoteDoctorProtocol, RemoteDoctorRepairResult,
    RepairRoundObservation, StoredRemoteDoctorIdentity, TargetLocation,
};
use crate::bridge_client::BridgeClient;
use crate::cli_runner::{get_active_openclaw_home_override, run_openclaw, run_openclaw_remote};
use crate::commands::logs::log_dev;
use crate::commands::{manage_rescue_bot, remote_manage_rescue_bot, RescuePrimaryDiagnosisResult};
use crate::node_client::NodeClient;
use crate::ssh::SshConnectionPool;

const MAX_REMOTE_DOCTOR_ROUNDS: usize = 50;
const REPAIR_PLAN_STALL_THRESHOLD: usize = 3;

async fn ensure_agent_bridge_connected<R: Runtime>(
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

async fn ensure_remote_target_connected(
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

fn is_unknown_method_error(error: &str) -> bool {
    error.contains("unknown method")
        || error.contains("\"code\":\"INVALID_REQUEST\"")
        || error.contains("\"code\": \"INVALID_REQUEST\"")
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

async fn repair_rescue_gateway_if_needed<R: Runtime>(
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

fn clawpal_server_step_type_summary(steps: &[ClawpalServerPlanStep]) -> Value {
    let mut counts = serde_json::Map::new();
    for step in steps {
        let entry = counts
            .entry(step.step_type.clone())
            .or_insert_with(|| Value::from(0_u64));
        let next = entry.as_u64().unwrap_or(0) + 1;
        *entry = Value::from(next);
    }
    Value::Object(counts)
}

fn repair_plan_stalled(observations: &[RepairRoundObservation], threshold: usize) -> bool {
    if observations.len() < threshold {
        return false;
    }
    let recent = &observations[observations.len() - threshold..];
    let Some(first) = recent.first() else {
        return false;
    };
    !first.issue_summaries.is_empty()
        && recent.iter().all(|entry| {
            entry.step_types.len() == 1
                && entry.step_types[0] == "doctorRediagnose"
                && entry.diagnosis_signature == first.diagnosis_signature
        })
}

fn round_limit_error_message(
    diagnosis: &RescuePrimaryDiagnosisResult,
    last_step_types: &[String],
) -> String {
    let issue_summary = serde_json::to_string(&diagnosis_issue_summaries(diagnosis))
        .unwrap_or_else(|_| "[]".to_string());
    let step_summary = if last_step_types.is_empty() {
        "[]".to_string()
    } else {
        serde_json::to_string(last_step_types).unwrap_or_else(|_| "[]".to_string())
    };
    format!(
        "Remote Doctor repair exceeded {MAX_REMOTE_DOCTOR_ROUNDS} rounds without a clean rescue diagnosis result. Last issues: {issue_summary}. Last repair step types: {step_summary}."
    )
}

fn stalled_plan_error_message(observation: &RepairRoundObservation) -> String {
    let issue_summary =
        serde_json::to_string(&observation.issue_summaries).unwrap_or_else(|_| "[]".to_string());
    let step_summary =
        serde_json::to_string(&observation.step_types).unwrap_or_else(|_| "[]".to_string());
    format!(
        "Remote Doctor did not return actionable repair steps by round {} after {} repeated rounds. Last issues: {}. Last repair step types: {}.",
        observation.round,
        REPAIR_PLAN_STALL_THRESHOLD,
        issue_summary,
        step_summary
    )
}

fn ensure_object(value: &mut Value) -> Result<&mut serde_json::Map<String, Value>, String> {
    if !value.is_object() {
        *value = json!({});
    }
    value
        .as_object_mut()
        .ok_or_else(|| "Expected object while applying remote doctor config step".to_string())
}

fn apply_config_set(root: &mut Value, path: &str, value: Value) -> Result<(), String> {
    let segments = path
        .split('.')
        .filter(|segment| !segment.trim().is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return Err("Config set path cannot be empty".into());
    }
    let mut cursor = root;
    for segment in &segments[..segments.len() - 1] {
        let object = ensure_object(cursor)?;
        cursor = object
            .entry((*segment).to_string())
            .or_insert_with(|| json!({}));
    }
    let object = ensure_object(cursor)?;
    object.insert(segments[segments.len() - 1].to_string(), value);
    Ok(())
}

fn apply_config_unset(root: &mut Value, path: &str) -> Result<(), String> {
    let segments = path
        .split('.')
        .filter(|segment| !segment.trim().is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return Err("Config unset path cannot be empty".into());
    }
    let mut cursor = root;
    for segment in &segments[..segments.len() - 1] {
        let Some(next) = cursor
            .as_object_mut()
            .and_then(|object| object.get_mut(*segment))
        else {
            return Ok(());
        };
        cursor = next;
    }
    if let Some(object) = cursor.as_object_mut() {
        object.remove(segments[segments.len() - 1]);
    }
    Ok(())
}

fn extract_json_block(text: &str) -> Option<&str> {
    clawpal_core::doctor::extract_json_from_output(text)
}

fn parse_agent_plan_response(kind: PlanKind, text: &str) -> Result<PlanResponse, String> {
    let json_block = extract_json_block(text)
        .ok_or_else(|| format!("Remote doctor agent did not return JSON: {text}"))?;
    let value: Value = serde_json::from_str(json_block)
        .map_err(|error| format!("Failed to parse remote doctor agent JSON: {error}"))?;
    parse_plan_response(kind, value)
}

fn parse_invoke_argv(command: &str, args: &Value) -> Result<Vec<String>, String> {
    if let Some(argv) = args.get("argv").and_then(Value::as_array) {
        let parsed = argv
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "invoke argv entries must be strings".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if parsed.is_empty() {
            return Err("invoke argv cannot be empty".into());
        }
        return Ok(parsed);
    }

    let arg_string = args
        .get("args")
        .and_then(Value::as_str)
        .or_else(|| args.get("command").and_then(Value::as_str))
        .unwrap_or("");
    let mut parsed = if arg_string.trim().is_empty() {
        Vec::new()
    } else {
        shell_words::split(arg_string)
            .map_err(|error| format!("Failed to parse invoke args: {error}"))?
    };
    if parsed.first().map(String::as_str) != Some(command) {
        parsed.insert(0, command.to_string());
    }
    Ok(parsed)
}

async fn execute_clawpal_command<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SshConnectionPool,
    target_location: TargetLocation,
    instance_id: &str,
    argv: &[String],
) -> Result<Value, String> {
    match argv.get(1).map(String::as_str) {
        Some("doctor") => {
            execute_clawpal_doctor_command(app, pool, target_location, instance_id, argv).await
        }
        other => Err(format!(
            "Unsupported clawpal command in remote doctor agent session: {:?}",
            other
        )),
    }
}

async fn execute_clawpal_doctor_command<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SshConnectionPool,
    target_location: TargetLocation,
    instance_id: &str,
    argv: &[String],
) -> Result<Value, String> {
    match argv.get(2).map(String::as_str) {
        Some("probe-openclaw") => {
            let version_result = execute_command(
                pool,
                target_location,
                instance_id,
                &["openclaw".into(), "--version".into()],
            )
            .await?;
            let which_result = match target_location {
                TargetLocation::LocalOpenclaw => {
                    execute_command(
                        pool,
                        target_location,
                        instance_id,
                        &[
                            "sh".into(),
                            "-lc".into(),
                            "command -v openclaw || true".into(),
                        ],
                    )
                    .await?
                }
                TargetLocation::RemoteOpenclaw => {
                    execute_command(
                        pool,
                        target_location,
                        instance_id,
                        &[
                            "sh".into(),
                            "-lc".into(),
                            "command -v openclaw || true".into(),
                        ],
                    )
                    .await?
                }
            };
            Ok(json!({
                "ok": version_result.exit_code == Some(0),
                "version": version_result.stdout.trim(),
                "openclawPath": which_result.stdout.trim(),
            }))
        }
        Some("config-read") => {
            let maybe_path = argv
                .get(3)
                .map(String::as_str)
                .filter(|value| !value.starts_with("--"));
            let raw = read_target_config_raw(app, target_location, instance_id).await?;
            config_read_response(&raw, maybe_path)
        }
        Some("config-read-raw") => {
            let raw = read_target_config_raw(app, target_location, instance_id).await?;
            Ok(json!({
                "raw": raw,
            }))
        }
        Some("config-delete") => {
            let path = argv
                .get(3)
                .ok_or("clawpal doctor config-delete requires a path")?;
            let mut config = read_target_config(app, target_location, instance_id).await?;
            apply_config_unset(&mut config, path)?;
            write_target_config(app, target_location, instance_id, &config).await?;
            restart_target_gateway(app, target_location, instance_id).await?;
            Ok(json!({ "deleted": true, "path": path }))
        }
        Some("config-write-raw-base64") => {
            let encoded = argv
                .get(3)
                .ok_or("clawpal doctor config-write-raw-base64 requires a base64 payload")?;
            let decoded = decode_base64_config_payload(encoded)?;
            write_target_config_raw(app, target_location, instance_id, &decoded).await?;
            restart_target_gateway(app, target_location, instance_id).await?;
            Ok(json!({
                "written": true,
                "bytes": decoded.len(),
            }))
        }
        Some("config-upsert") => {
            let path = argv
                .get(3)
                .ok_or("clawpal doctor config-upsert requires a path")?;
            let value_raw = argv
                .get(4)
                .ok_or("clawpal doctor config-upsert requires a value")?;
            let value: Value = serde_json::from_str(value_raw)
                .map_err(|error| format!("Invalid JSON value for config-upsert: {error}"))?;
            let mut config = read_target_config(app, target_location, instance_id).await?;
            apply_config_set(&mut config, path, value)?;
            write_target_config(app, target_location, instance_id, &config).await?;
            restart_target_gateway(app, target_location, instance_id).await?;
            Ok(json!({ "upserted": true, "path": path }))
        }
        Some("exec") => {
            let tool_idx = argv
                .iter()
                .position(|part| part == "--tool")
                .ok_or("clawpal doctor exec requires --tool")?;
            let tool = argv
                .get(tool_idx + 1)
                .ok_or("clawpal doctor exec missing tool name")?;
            let args_idx = argv.iter().position(|part| part == "--args");
            let mut exec_argv = vec![tool.clone()];
            if let Some(index) = args_idx {
                if let Some(arg_string) = argv.get(index + 1) {
                    exec_argv.extend(shell_words::split(arg_string).map_err(|error| {
                        format!("Failed to parse clawpal doctor exec args: {error}")
                    })?);
                }
            }
            let result = execute_command(pool, target_location, instance_id, &exec_argv).await?;
            Ok(json!({
                "argv": result.argv,
                "exitCode": result.exit_code,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }))
        }
        other => Err(format!(
            "Unsupported clawpal doctor subcommand in remote doctor agent session: {:?}",
            other
        )),
    }
}

fn config_read_response(raw: &str, path: Option<&str>) -> Result<Value, String> {
    let context = build_config_excerpt_context(raw);
    if let Some(parse_error) = context.config_parse_error {
        return Ok(json!({
            "value": Value::Null,
            "path": path,
            "raw": context.config_excerpt_raw.unwrap_or_else(|| raw.to_string()),
            "parseError": parse_error,
        }));
    }

    let value = if let Some(path) = path {
        clawpal_core::doctor::select_json_value_from_str(
            &serde_json::to_string_pretty(&context.config_excerpt).unwrap_or_else(|_| "{}".into()),
            Some(path),
            "remote doctor config",
        )?
    } else {
        context.config_excerpt
    };

    Ok(json!({
        "value": value,
        "path": path,
    }))
}

fn decode_base64_config_payload(encoded: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|error| format!("Failed to decode base64 config payload: {error}"))?;
    String::from_utf8(bytes)
        .map_err(|error| format!("Base64 config payload is not valid UTF-8: {error}"))
}

async fn execute_invoke_payload<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SshConnectionPool,
    target_location: TargetLocation,
    instance_id: &str,
    payload: &Value,
) -> Result<Value, String> {
    let command = payload
        .get("command")
        .and_then(Value::as_str)
        .ok_or("invoke payload missing command")?;
    let args = payload.get("args").cloned().unwrap_or(Value::Null);
    let argv = parse_invoke_argv(command, &args)?;
    match command {
        "openclaw" => {
            let result = execute_command(pool, target_location, instance_id, &argv).await?;
            Ok(json!({
                "argv": result.argv,
                "exitCode": result.exit_code,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }))
        }
        "clawpal" => execute_clawpal_command(app, pool, target_location, instance_id, &argv).await,
        other => Err(format!(
            "Unsupported invoke command in remote doctor agent session: {other}"
        )),
    }
}

async fn run_agent_request_with_bridge<R: Runtime>(
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

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn build_shell_command(argv: &[String]) -> String {
    argv.iter()
        .map(|part| shell_escape(part))
        .collect::<Vec<String>>()
        .join(" ")
}

async fn execute_command(
    pool: &SshConnectionPool,
    target_location: TargetLocation,
    instance_id: &str,
    argv: &[String],
) -> Result<CommandResult, String> {
    let started = Instant::now();
    if argv.is_empty() {
        return Err("Plan command argv cannot be empty".into());
    }
    let result = match target_location {
        TargetLocation::LocalOpenclaw => {
            if argv[0] == "openclaw" {
                let arg_refs = argv
                    .iter()
                    .skip(1)
                    .map(String::as_str)
                    .collect::<Vec<&str>>();
                let output = run_openclaw(&arg_refs)?;
                CommandResult {
                    argv: argv.to_vec(),
                    exit_code: Some(output.exit_code),
                    stdout: output.stdout,
                    stderr: output.stderr,
                    duration_ms: started.elapsed().as_millis() as u64,
                    timed_out: false,
                }
            } else {
                let mut command = std::process::Command::new(&argv[0]);
                command.args(argv.iter().skip(1));
                if let Some(openclaw_home) = get_active_openclaw_home_override() {
                    command.env("OPENCLAW_HOME", openclaw_home);
                }
                let output = command.output().map_err(|error| {
                    format!("Failed to execute local command {:?}: {error}", argv)
                })?;
                CommandResult {
                    argv: argv.to_vec(),
                    exit_code: output.status.code(),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    duration_ms: started.elapsed().as_millis() as u64,
                    timed_out: false,
                }
            }
        }
        TargetLocation::RemoteOpenclaw => {
            let host_id = primary_remote_target_host_id(instance_id)?;
            if argv[0] == "openclaw" {
                let arg_refs = argv
                    .iter()
                    .skip(1)
                    .map(String::as_str)
                    .collect::<Vec<&str>>();
                let output = run_openclaw_remote(pool, &host_id, &arg_refs).await?;
                CommandResult {
                    argv: argv.to_vec(),
                    exit_code: Some(output.exit_code),
                    stdout: output.stdout,
                    stderr: output.stderr,
                    duration_ms: started.elapsed().as_millis() as u64,
                    timed_out: false,
                }
            } else {
                let output = pool
                    .exec_login(&host_id, &build_shell_command(argv))
                    .await?;
                CommandResult {
                    argv: argv.to_vec(),
                    exit_code: Some(output.exit_code as i32),
                    stdout: output.stdout,
                    stderr: output.stderr,
                    duration_ms: started.elapsed().as_millis() as u64,
                    timed_out: false,
                }
            }
        }
    };
    Ok(result)
}

fn plan_command_uses_internal_clawpal_tool(argv: &[String]) -> bool {
    argv.first().map(String::as_str) == Some("clawpal")
}

fn validate_clawpal_exec_args(argv: &[String]) -> Result<(), String> {
    if argv.get(0).map(String::as_str) != Some("clawpal")
        || argv.get(1).map(String::as_str) != Some("doctor")
        || argv.get(2).map(String::as_str) != Some("exec")
    {
        return Ok(());
    }

    let args_idx = argv.iter().position(|part| part == "--args");
    let Some(index) = args_idx else {
        return Ok(());
    };
    let Some(arg_string) = argv.get(index + 1) else {
        return Ok(());
    };
    if arg_string.contains('\n') || arg_string.contains("<<") {
        return Err(format!(
            "Unsupported clawpal doctor exec args: {}. Use bounded single-line commands without heredocs or stdin-driven scripts.",
            argv.join(" ")
        ));
    }
    Ok(())
}

fn validate_plan_command_argv(argv: &[String]) -> Result<(), String> {
    if argv.is_empty() {
        return Err("Plan command argv cannot be empty".into());
    }
    validate_clawpal_exec_args(argv)?;
    if argv[0] != "openclaw" {
        return Ok(());
    }

    let supported = argv == ["openclaw", "--version"] || argv == ["openclaw", "gateway", "status"];
    if supported {
        Ok(())
    } else {
        Err(format!(
            "Unsupported openclaw plan command: {}",
            argv.join(" ")
        ))
    }
}

fn plan_command_failure_message(
    kind: PlanKind,
    round: usize,
    argv: &[String],
    error: &str,
) -> String {
    let kind_label = match kind {
        PlanKind::Detect => "Detect",
        PlanKind::Investigate => "Investigate",
        PlanKind::Repair => "Repair",
    };
    format!(
        "{kind_label} command failed in round {round}: {}: {error}",
        argv.join(" ")
    )
}

fn command_result_stdout(value: &Value) -> String {
    value
        .get("stdout")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        })
}

async fn execute_plan_command<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SshConnectionPool,
    target_location: TargetLocation,
    instance_id: &str,
    argv: &[String],
) -> Result<CommandResult, String> {
    let started = Instant::now();
    validate_plan_command_argv(argv)?;
    if plan_command_uses_internal_clawpal_tool(argv) {
        let value = execute_clawpal_command(app, pool, target_location, instance_id, argv).await?;
        let exit_code = value
            .get("exitCode")
            .and_then(Value::as_i64)
            .map(|code| code as i32)
            .unwrap_or(0);
        let stderr = value
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        return Ok(CommandResult {
            argv: argv.to_vec(),
            exit_code: Some(exit_code),
            stdout: command_result_stdout(&value),
            stderr,
            duration_ms: started.elapsed().as_millis() as u64,
            timed_out: false,
        });
    }

    execute_command(pool, target_location, instance_id, argv).await
}

fn parse_plan_response(kind: PlanKind, value: Value) -> Result<PlanResponse, String> {
    let mut response: PlanResponse = serde_json::from_value(value)
        .map_err(|error| format!("Failed to parse remote doctor plan response: {error}"))?;
    response.plan_kind = kind;
    if response.plan_id.trim().is_empty() {
        response.plan_id = format!("plan-{}", Uuid::new_v4());
    }
    Ok(response)
}

async fn request_plan(
    client: &NodeClient,
    method: &str,
    kind: PlanKind,
    session_id: &str,
    round: usize,
    target_location: TargetLocation,
    instance_id: &str,
    previous_results: &[CommandResult],
) -> Result<PlanResponse, String> {
    let response = client
        .send_request(
            method,
            json!({
                "sessionId": session_id,
                "round": round,
                "planKind": match kind {
                    PlanKind::Detect => "detect",
                    PlanKind::Investigate => "investigate",
                    PlanKind::Repair => "repair",
                },
                "targetLocation": match target_location {
                    TargetLocation::LocalOpenclaw => "local_openclaw",
                    TargetLocation::RemoteOpenclaw => "remote_openclaw",
                },
                "instanceId": instance_id,
                "hostId": instance_id.strip_prefix("ssh:"),
                "previousResults": previous_results,
            }),
        )
        .await?;
    parse_plan_response(kind, response)
}

async fn request_agent_plan<R: Runtime>(
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

fn agent_plan_step_types(plan: &PlanResponse) -> Vec<String> {
    if plan.commands.is_empty() {
        return vec![format!(
            "plan:{}",
            match plan.plan_kind {
                PlanKind::Detect => "detect",
                PlanKind::Investigate => "investigate",
                PlanKind::Repair => "repair",
            }
        )];
    }
    plan.commands
        .iter()
        .map(|command| {
            command
                .argv
                .first()
                .cloned()
                .unwrap_or_else(|| "empty-command".to_string())
        })
        .collect()
}

async fn request_clawpal_server_plan(
    client: &NodeClient,
    session_id: &str,
    round: usize,
    instance_id: &str,
    target_location: TargetLocation,
    diagnosis: &RescuePrimaryDiagnosisResult,
    config_context: &ConfigExcerptContext,
) -> Result<ClawpalServerPlanResponse, String> {
    let response = client
        .send_request(
            "remote_repair_plan.request",
            json!({
                "requestId": format!("{session_id}-round-{round}"),
                "targetId": instance_id,
                "targetLocation": match target_location {
                    TargetLocation::LocalOpenclaw => "local_openclaw",
                    TargetLocation::RemoteOpenclaw => "remote_openclaw",
                },
                "context": {
                    "configExcerpt": config_context.config_excerpt,
                    "configExcerptRaw": config_context.config_excerpt_raw,
                    "configParseError": config_context.config_parse_error,
                    "diagnosis": diagnosis_context(diagnosis),
                }
            }),
        )
        .await?;
    serde_json::from_value::<ClawpalServerPlanResponse>(response)
        .map_err(|error| format!("Failed to parse clawpal-server plan response: {error}"))
}

async fn report_clawpal_server_step_result(
    client: &NodeClient,
    plan_id: &str,
    step_index: usize,
    step: &ClawpalServerPlanStep,
    result: &CommandResult,
) {
    let _ = client
        .send_request(
            "remote_repair_plan.step_result",
            json!({
                "planId": plan_id,
                "stepIndex": step_index,
                "step": step,
                "result": result,
            }),
        )
        .await;
}

async fn report_clawpal_server_final_result(
    client: &NodeClient,
    plan_id: &str,
    healthy: bool,
    diagnosis: &RescuePrimaryDiagnosisResult,
) {
    let _ = client
        .send_request(
            "remote_repair_plan.final_result",
            json!({
                "planId": plan_id,
                "healthy": healthy,
                "diagnosis": diagnosis_context(diagnosis),
            }),
        )
        .await;
}

async fn run_remote_doctor_repair_loop<R: Runtime, F, Fut>(
    app: Option<&AppHandle<R>>,
    pool: &SshConnectionPool,
    session_id: &str,
    instance_id: &str,
    target_location: TargetLocation,
    mut request_plan_fn: F,
) -> Result<RemoteDoctorRepairResult, String>
where
    F: FnMut(PlanKind, usize, Vec<CommandResult>) -> Fut,
    Fut: std::future::Future<Output = Result<PlanResponse, String>>,
{
    let mut previous_results: Vec<CommandResult> = Vec::new();
    let mut last_command: Option<Vec<String>> = None;
    let mut last_plan_kind = PlanKind::Detect;

    for round in 1..=MAX_REMOTE_DOCTOR_ROUNDS {
        emit_progress(
            app,
            session_id,
            round,
            "planning_detect",
            format!("Requesting detection plan for round {round}"),
            Some(PlanKind::Detect),
            None,
        );
        let detect_plan =
            request_plan_fn(PlanKind::Detect, round, previous_results.clone()).await?;
        append_remote_doctor_log(
            session_id,
            json!({
                "event": "plan_received",
                "round": round,
                "planKind": "detect",
                "planId": detect_plan.plan_id,
                "summary": detect_plan.summary,
                "commandCount": detect_plan.commands.len(),
                "healthy": detect_plan.healthy,
                "done": detect_plan.done,
            }),
        );
        if detect_plan.healthy || (detect_plan.done && detect_plan.commands.is_empty()) {
            return Ok(RemoteDoctorRepairResult {
                mode: "remoteDoctor".into(),
                status: "completed".into(),
                round,
                phase: "completed".into(),
                last_plan_kind: match last_plan_kind {
                    PlanKind::Detect => "detect".into(),
                    PlanKind::Investigate => "investigate".into(),
                    PlanKind::Repair => "repair".into(),
                },
                latest_diagnosis_healthy: true,
                last_command,
                session_id: session_id.to_string(),
                message: "Remote Doctor repair completed with a healthy detection result.".into(),
            });
        }
        previous_results.clear();
        for command in &detect_plan.commands {
            last_command = Some(command.argv.clone());
            emit_progress(
                app,
                session_id,
                round,
                "executing_detect",
                format!("Running detect command: {}", command.argv.join(" ")),
                Some(PlanKind::Detect),
                Some(command.argv.clone()),
            );
            let command_result =
                execute_command(pool, target_location, instance_id, &command.argv).await?;
            append_remote_doctor_log(
                session_id,
                json!({
                    "event": "command_result",
                    "round": round,
                    "planKind": "detect",
                    "result": command_result,
                }),
            );
            if command_result.exit_code.unwrap_or(1) != 0
                && !command.continue_on_failure.unwrap_or(false)
            {
                previous_results.push(command_result);
                return Err(format!(
                    "Detect command failed in round {round}: {}",
                    command.argv.join(" ")
                ));
            }
            previous_results.push(command_result);
        }

        emit_progress(
            app,
            session_id,
            round,
            "planning_repair",
            format!("Requesting repair plan for round {round}"),
            Some(PlanKind::Repair),
            None,
        );
        let repair_plan =
            request_plan_fn(PlanKind::Repair, round, previous_results.clone()).await?;
        last_plan_kind = PlanKind::Repair;
        append_remote_doctor_log(
            session_id,
            json!({
                "event": "plan_received",
                "round": round,
                "planKind": "repair",
                "planId": repair_plan.plan_id,
                "summary": repair_plan.summary,
                "commandCount": repair_plan.commands.len(),
                "success": repair_plan.success,
                "done": repair_plan.done,
            }),
        );
        previous_results.clear();
        for command in &repair_plan.commands {
            last_command = Some(command.argv.clone());
            emit_progress(
                app,
                session_id,
                round,
                "executing_repair",
                format!("Running repair command: {}", command.argv.join(" ")),
                Some(PlanKind::Repair),
                Some(command.argv.clone()),
            );
            let command_result =
                execute_command(pool, target_location, instance_id, &command.argv).await?;
            append_remote_doctor_log(
                session_id,
                json!({
                    "event": "command_result",
                    "round": round,
                    "planKind": "repair",
                    "result": command_result,
                }),
            );
            if command_result.exit_code.unwrap_or(1) != 0
                && !command.continue_on_failure.unwrap_or(false)
            {
                previous_results.push(command_result);
                return Err(format!(
                    "Repair command failed in round {round}: {}",
                    command.argv.join(" ")
                ));
            }
            previous_results.push(command_result);
        }
    }

    append_remote_doctor_log(
        session_id,
        json!({
            "event": "session_complete",
            "status": "failed",
            "reason": "round_limit_exceeded",
        }),
    );
    Err(format!(
        "Remote Doctor repair exceeded {MAX_REMOTE_DOCTOR_ROUNDS} rounds without a clean detection result"
    ))
}

async fn run_clawpal_server_repair_loop<R: Runtime>(
    app: &AppHandle<R>,
    client: &NodeClient,
    session_id: &str,
    instance_id: &str,
    target_location: TargetLocation,
) -> Result<RemoteDoctorRepairResult, String> {
    let mut diagnosis = run_rescue_diagnosis(app, target_location, instance_id).await?;
    append_diagnosis_log(session_id, "initial", 0, &diagnosis);
    if protocol_runs_rescue_preflight(RemoteDoctorProtocol::ClawpalServer) {
        repair_rescue_gateway_if_needed(
            app,
            session_id,
            0,
            target_location,
            instance_id,
            &mut diagnosis,
        )
        .await?;
    }
    if diagnosis_is_healthy(&diagnosis) {
        return Ok(result_for_completion(
            session_id,
            0,
            PlanKind::Detect,
            None,
            "Remote Doctor repair skipped because diagnosis is already healthy.",
        ));
    }

    let mut last_command = None;
    let mut round_observations: Vec<RepairRoundObservation> = Vec::new();
    let mut last_step_types: Vec<String> = Vec::new();
    for round in 1..=MAX_REMOTE_DOCTOR_ROUNDS {
        emit_progress(
            Some(app),
            session_id,
            round,
            "planning_repair",
            format!("Requesting remote repair plan for round {round}"),
            Some(PlanKind::Repair),
            None,
        );
        let config_context = build_config_excerpt_context(
            &read_target_config_raw(app, target_location, instance_id).await?,
        );
        append_remote_doctor_log(
            session_id,
            json!({
                "event": "plan_request_context",
                "protocol": "clawpal_server",
                "round": round,
                "planKind": "repair",
                "instanceId": instance_id,
                "targetLocation": target_location,
                "configContext": config_excerpt_log_summary(&config_context),
                "diagnosisIssueCount": diagnosis.issues.len(),
                "diagnosisIssues": diagnosis_issue_summaries(&diagnosis),
            }),
        );
        if config_context.config_parse_error.is_some() {
            append_remote_doctor_log(
                session_id,
                json!({
                    "event": "config_recovery_context",
                    "round": round,
                    "context": config_excerpt_log_summary(&config_context),
                }),
            );
        }
        let plan = request_clawpal_server_plan(
            client,
            session_id,
            round,
            instance_id,
            target_location,
            &diagnosis,
            &config_context,
        )
        .await?;
        append_remote_doctor_log(
            session_id,
            json!({
                "event": "plan_received",
                "protocol": "clawpal_server",
                "round": round,
                "planKind": "repair",
                "planId": plan.plan_id,
                "summary": plan.summary,
                "stepCount": plan.steps.len(),
                "stepTypeCounts": clawpal_server_step_type_summary(&plan.steps),
            }),
        );

        let mut current_config = config_context.config_excerpt.clone();
        let mut rediagnosed = false;
        let mut round_step_types = Vec::new();
        for (step_index, step) in plan.steps.iter().enumerate() {
            round_step_types.push(step.step_type.clone());
            let mut result = CommandResult {
                argv: Vec::new(),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: 0,
                timed_out: false,
            };
            let started = Instant::now();
            match step.step_type.as_str() {
                "configSet" => {
                    let path = step.path.as_deref().ok_or("configSet step missing path")?;
                    let value = step.value.clone().ok_or("configSet step missing value")?;
                    emit_progress(
                        Some(app),
                        session_id,
                        round,
                        "executing_repair",
                        format!("Applying config set: {path}"),
                        Some(PlanKind::Repair),
                        None,
                    );
                    apply_config_set(&mut current_config, path, value)?;
                    write_target_config(app, target_location, instance_id, &current_config).await?;
                    restart_target_gateway(app, target_location, instance_id).await?;
                    result.argv = vec!["configSet".into(), path.into()];
                    result.stdout = format!("Updated {path}");
                }
                "configUnset" => {
                    let path = step
                        .path
                        .as_deref()
                        .ok_or("configUnset step missing path")?;
                    emit_progress(
                        Some(app),
                        session_id,
                        round,
                        "executing_repair",
                        format!("Applying config unset: {path}"),
                        Some(PlanKind::Repair),
                        None,
                    );
                    apply_config_unset(&mut current_config, path)?;
                    write_target_config(app, target_location, instance_id, &current_config).await?;
                    restart_target_gateway(app, target_location, instance_id).await?;
                    result.argv = vec!["configUnset".into(), path.into()];
                    result.stdout = format!("Removed {path}");
                }
                "doctorRediagnose" => {
                    emit_progress(
                        Some(app),
                        session_id,
                        round,
                        "planning_detect",
                        format!("Running rescue diagnosis after repair plan round {round}"),
                        Some(PlanKind::Detect),
                        None,
                    );
                    diagnosis = run_rescue_diagnosis(app, target_location, instance_id).await?;
                    append_diagnosis_log(session_id, "post_step_rediagnose", round, &diagnosis);
                    rediagnosed = true;
                    result.argv = vec!["doctorRediagnose".into()];
                    result.stdout = format!(
                        "Diagnosis status={} issues={}",
                        diagnosis.status,
                        diagnosis.issues.len()
                    );
                }
                other => {
                    result.exit_code = Some(1);
                    result.stderr = format!("Unsupported clawpal-server step type: {other}");
                }
            }
            result.duration_ms = started.elapsed().as_millis() as u64;
            last_command = Some(result.argv.clone());
            append_remote_doctor_log(
                session_id,
                json!({
                    "event": "command_result",
                    "protocol": "clawpal_server",
                    "round": round,
                    "planKind": "repair",
                    "stepIndex": step_index,
                    "step": step,
                    "result": result,
                }),
            );
            report_clawpal_server_step_result(client, &plan.plan_id, step_index, step, &result)
                .await;
            if result.exit_code.unwrap_or(1) != 0 {
                return Err(result.stderr);
            }
        }

        if !rediagnosed {
            diagnosis = run_rescue_diagnosis(app, target_location, instance_id).await?;
            append_diagnosis_log(session_id, "post_round", round, &diagnosis);
        }
        if protocol_runs_rescue_preflight(RemoteDoctorProtocol::ClawpalServer) {
            repair_rescue_gateway_if_needed(
                app,
                session_id,
                round,
                target_location,
                instance_id,
                &mut diagnosis,
            )
            .await?;
        }
        last_step_types = round_step_types.clone();
        round_observations.push(RepairRoundObservation::new(
            round,
            &round_step_types,
            &diagnosis,
        ));
        if repair_plan_stalled(&round_observations, REPAIR_PLAN_STALL_THRESHOLD) {
            let observation = round_observations
                .last()
                .expect("stalled observations should contain current round");
            append_remote_doctor_log(
                session_id,
                json!({
                    "event": "repair_plan_stalled",
                    "protocol": "clawpal_server",
                    "round": round,
                    "repeatedRounds": REPAIR_PLAN_STALL_THRESHOLD,
                    "latestStepTypes": observation.step_types,
                    "issues": observation.issue_summaries,
                }),
            );
            return Err(stalled_plan_error_message(observation));
        }
        let healthy = diagnosis_is_healthy(&diagnosis);
        report_clawpal_server_final_result(client, &plan.plan_id, healthy, &diagnosis).await;
        if healthy {
            return Ok(result_for_completion(
                session_id,
                round,
                PlanKind::Repair,
                last_command,
                "Remote Doctor repair completed with a healthy rescue diagnosis.",
            ));
        }
    }

    Err(round_limit_error_message(&diagnosis, &last_step_types))
}

async fn run_agent_planner_repair_loop<R: Runtime>(
    app: &AppHandle<R>,
    client: &NodeClient,
    bridge_client: &BridgeClient,
    pool: &SshConnectionPool,
    session_id: &str,
    instance_id: &str,
    target_location: TargetLocation,
) -> Result<RemoteDoctorRepairResult, String> {
    let mut diagnosis = run_rescue_diagnosis(app, target_location, instance_id).await?;
    append_diagnosis_log(session_id, "initial", 0, &diagnosis);
    if diagnosis_is_healthy(&diagnosis) {
        return Ok(result_for_completion(
            session_id,
            0,
            PlanKind::Detect,
            None,
            "Remote Doctor repair skipped because diagnosis is already healthy.",
        ));
    }

    let mut previous_results: Vec<CommandResult> = Vec::new();
    let mut last_command = None;
    let mut last_step_types: Vec<String> = Vec::new();
    let mut round_observations: Vec<RepairRoundObservation> = Vec::new();

    for round in 1..=MAX_REMOTE_DOCTOR_ROUNDS {
        let kind = next_agent_plan_kind_for_round(&diagnosis, &previous_results);
        let config_context = build_config_excerpt_context(
            &read_target_config_raw(app, target_location, instance_id).await?,
        );
        let phase = match kind {
            PlanKind::Detect => "planning_detect",
            PlanKind::Investigate => "planning_investigate",
            PlanKind::Repair => "planning_repair",
        };
        let line = match kind {
            PlanKind::Detect => format!("Requesting detection plan for round {round}"),
            PlanKind::Investigate => format!("Requesting investigation plan for round {round}"),
            PlanKind::Repair => format!("Requesting repair plan for round {round}"),
        };
        emit_progress(Some(app), session_id, round, phase, line, Some(kind), None);
        append_remote_doctor_log(
            session_id,
            json!({
                "event": "plan_request_context",
                "protocol": "agent",
                "round": round,
                "planKind": match kind {
                    PlanKind::Detect => "detect",
                    PlanKind::Investigate => "investigate",
                    PlanKind::Repair => "repair",
                },
                "instanceId": instance_id,
                "targetLocation": target_location,
                "configContext": config_excerpt_log_summary(&config_context),
                "diagnosisIssueCount": diagnosis.issues.len(),
                "diagnosisIssues": diagnosis_issue_summaries(&diagnosis),
            }),
        );
        let plan = request_agent_plan(
            app,
            client,
            bridge_client,
            pool,
            session_id,
            round,
            kind,
            target_location,
            instance_id,
            &diagnosis,
            &config_context,
            &previous_results,
        )
        .await?;
        append_remote_doctor_log(
            session_id,
            json!({
                "event": "plan_received",
                "protocol": "agent",
                "round": round,
                "planKind": match plan.plan_kind {
                    PlanKind::Detect => "detect",
                    PlanKind::Investigate => "investigate",
                    PlanKind::Repair => "repair",
                },
                "planId": plan.plan_id,
                "summary": plan.summary,
                "commandCount": plan.commands.len(),
                "healthy": plan.healthy,
                "done": plan.done,
                "success": plan.success,
            }),
        );
        previous_results.clear();
        last_step_types = agent_plan_step_types(&plan);
        for command in &plan.commands {
            last_command = Some(command.argv.clone());
            emit_progress(
                Some(app),
                session_id,
                round,
                match kind {
                    PlanKind::Detect => "executing_detect",
                    PlanKind::Investigate => "executing_investigate",
                    PlanKind::Repair => "executing_repair",
                },
                format!(
                    "Running {} command: {}",
                    match kind {
                        PlanKind::Detect => "detect",
                        PlanKind::Investigate => "investigate",
                        PlanKind::Repair => "repair",
                    },
                    command.argv.join(" ")
                ),
                Some(kind),
                Some(command.argv.clone()),
            );
            append_remote_doctor_log(
                session_id,
                json!({
                    "event": "command_start",
                    "round": round,
                    "planKind": match kind {
                        PlanKind::Detect => "detect",
                        PlanKind::Investigate => "investigate",
                        PlanKind::Repair => "repair",
                    },
                    "argv": command.argv,
                    "timeoutSec": command.timeout_sec,
                    "purpose": command.purpose,
                }),
            );
            let command_result =
                match execute_plan_command(app, pool, target_location, instance_id, &command.argv)
                    .await
                {
                    Ok(result) => result,
                    Err(error) => {
                        return Err(plan_command_failure_message(
                            kind,
                            round,
                            &command.argv,
                            &error,
                        ));
                    }
                };
            append_remote_doctor_log(
                session_id,
                json!({
                    "event": "command_result",
                    "round": round,
                    "planKind": match kind {
                        PlanKind::Detect => "detect",
                        PlanKind::Investigate => "investigate",
                        PlanKind::Repair => "repair",
                    },
                    "result": command_result,
                }),
            );
            if command_result.exit_code.unwrap_or(1) != 0
                && !command.continue_on_failure.unwrap_or(false)
            {
                return Err(format!(
                    "{} command failed in round {round}: {}",
                    match kind {
                        PlanKind::Detect => "Detect",
                        PlanKind::Investigate => "Investigate",
                        PlanKind::Repair => "Repair",
                    },
                    command.argv.join(" ")
                ));
            }
            previous_results.push(command_result);
        }

        diagnosis = run_rescue_diagnosis(app, target_location, instance_id).await?;
        append_diagnosis_log(session_id, "post_round", round, &diagnosis);
        if diagnosis_is_healthy(&diagnosis) {
            return Ok(result_for_completion(
                session_id,
                round,
                kind,
                last_command,
                "Remote Doctor repair completed with a healthy rescue diagnosis.",
            ));
        }
        if matches!(kind, PlanKind::Repair)
            && plan.done
            && plan.commands.is_empty()
            && diagnosis_has_only_non_auto_fixable_issues(&diagnosis)
        {
            return Ok(result_for_completion_with_warnings(
                session_id,
                round,
                kind,
                last_command,
                "Remote Doctor completed all safe automatic repairs. Remaining issues are non-auto-fixable warnings.",
            ));
        }

        round_observations.push(RepairRoundObservation::new(
            round,
            &last_step_types,
            &diagnosis,
        ));
        if repair_plan_stalled(&round_observations, REPAIR_PLAN_STALL_THRESHOLD) {
            let observation = round_observations
                .last()
                .expect("stalled observations should contain current round");
            append_remote_doctor_log(
                session_id,
                json!({
                    "event": "repair_plan_stalled",
                    "protocol": "agent",
                    "round": round,
                    "repeatedRounds": REPAIR_PLAN_STALL_THRESHOLD,
                    "latestStepTypes": observation.step_types,
                    "issues": observation.issue_summaries,
                }),
            );
            return Err(stalled_plan_error_message(observation));
        }
    }

    Err(round_limit_error_message(&diagnosis, &last_step_types))
}

async fn start_remote_doctor_repair_impl<R: Runtime>(
    app: AppHandle<R>,
    pool: &SshConnectionPool,
    instance_id: String,
    target_location: String,
) -> Result<RemoteDoctorRepairResult, String> {
    let target_location = parse_target_location(&target_location)?;
    if matches!(target_location, TargetLocation::RemoteOpenclaw) {
        ensure_remote_target_connected(pool, &instance_id).await?;
    }
    let session_id = Uuid::new_v4().to_string();
    let gateway = remote_doctor_gateway_config()?;
    let creds = remote_doctor_gateway_credentials(gateway.auth_token_override.as_deref())?;
    log_dev(format!(
        "[remote_doctor] start session={} instance_id={} target_location={:?} gateway_url={} auth_token_override={}",
        session_id,
        instance_id,
        target_location,
        gateway.url,
        gateway.auth_token_override.is_some()
    ));
    append_remote_doctor_log(
        &session_id,
        json!({
            "event": "session_start",
            "instanceId": instance_id,
            "targetLocation": target_location,
            "gatewayUrl": gateway.url,
            "gatewayAuthTokenOverride": gateway.auth_token_override.is_some(),
        }),
    );

    let client = NodeClient::new();
    client.connect(&gateway.url, app.clone(), creds).await?;
    let bridge = BridgeClient::new();

    let forced_protocol = configured_remote_doctor_protocol();
    let active_protocol = forced_protocol.unwrap_or(default_remote_doctor_protocol());
    let pool_ref: &SshConnectionPool = pool;
    let app_handle = app.clone();
    let bridge_client = bridge.clone();
    let gateway_url = gateway.url.clone();
    let gateway_auth_override = gateway.auth_token_override.clone();
    if matches!(active_protocol, RemoteDoctorProtocol::AgentPlanner)
        && gateway_url_is_local(&gateway_url)
    {
        ensure_local_remote_doctor_agent_ready()?;
    }
    if protocol_requires_bridge(active_protocol) {
        ensure_agent_bridge_connected(
            &app,
            &bridge,
            &gateway_url,
            gateway_auth_override.as_deref(),
            &session_id,
        )
        .await;
    }
    let result = match active_protocol {
        RemoteDoctorProtocol::AgentPlanner => {
            let agent = run_agent_planner_repair_loop(
                &app,
                &client,
                &bridge_client,
                pool_ref,
                &session_id,
                &instance_id,
                target_location,
            )
            .await;

            if forced_protocol.is_none()
                && matches!(&agent, Err(error) if is_unknown_method_error(error))
            {
                append_remote_doctor_log(
                    &session_id,
                    json!({
                        "event": "protocol_fallback",
                        "from": "agent",
                        "to": "legacy_doctor",
                        "reason": agent.as_ref().err(),
                    }),
                );
                run_remote_doctor_repair_loop(
                    Some(&app),
                    pool_ref,
                    &session_id,
                    &instance_id,
                    target_location,
                    |kind, round, previous_results| {
                        let method = match kind {
                            PlanKind::Detect => detect_method_name(),
                            PlanKind::Investigate => repair_method_name(),
                            PlanKind::Repair => repair_method_name(),
                        };
                        let client = &client;
                        let session_id = &session_id;
                        let instance_id = &instance_id;
                        async move {
                            request_plan(
                                client,
                                &method,
                                kind,
                                session_id,
                                round,
                                target_location,
                                instance_id,
                                &previous_results,
                            )
                            .await
                        }
                    },
                )
                .await
            } else {
                agent
            }
        }
        RemoteDoctorProtocol::LegacyDoctor => {
            let legacy = run_remote_doctor_repair_loop(
                Some(&app),
                pool_ref,
                &session_id,
                &instance_id,
                target_location,
                |kind, round, previous_results| {
                    let method = match kind {
                        PlanKind::Detect => detect_method_name(),
                        PlanKind::Investigate => repair_method_name(),
                        PlanKind::Repair => repair_method_name(),
                    };
                    let client = &client;
                    let session_id = &session_id;
                    let instance_id = &instance_id;
                    async move {
                        request_plan(
                            client,
                            &method,
                            kind,
                            session_id,
                            round,
                            target_location,
                            instance_id,
                            &previous_results,
                        )
                        .await
                    }
                },
            )
            .await;

            if forced_protocol.is_none()
                && matches!(&legacy, Err(error) if is_unknown_method_error(error))
            {
                append_remote_doctor_log(
                    &session_id,
                    json!({
                        "event": "protocol_fallback",
                        "from": "legacy_doctor",
                        "to": "clawpal_server",
                        "reason": legacy.as_ref().err(),
                    }),
                );
                log_dev(format!(
                    "[remote_doctor] session={} protocol fallback legacy_doctor -> clawpal_server",
                    session_id
                ));
                run_clawpal_server_repair_loop(
                    &app,
                    &client,
                    &session_id,
                    &instance_id,
                    target_location,
                )
                .await
            } else {
                legacy
            }
        }
        RemoteDoctorProtocol::ClawpalServer => {
            let clawpal_server = run_clawpal_server_repair_loop(
                &app,
                &client,
                &session_id,
                &instance_id,
                target_location,
            )
            .await;
            if forced_protocol.is_none()
                && matches!(&clawpal_server, Err(error) if is_unknown_method_error(error))
            {
                append_remote_doctor_log(
                    &session_id,
                    json!({
                        "event": "protocol_fallback",
                        "from": "clawpal_server",
                        "to": "agent",
                        "reason": clawpal_server.as_ref().err(),
                    }),
                );
                let agent = run_remote_doctor_repair_loop(
                    Some(&app),
                    pool_ref,
                    &session_id,
                    &instance_id,
                    target_location,
                    |kind, round, previous_results| {
                        let client = &client;
                        let session_id = &session_id;
                        let instance_id = &instance_id;
                        let app_handle = app_handle.clone();
                        let bridge_client = bridge_client.clone();
                        let gateway_url = gateway_url.clone();
                        let gateway_auth_override = gateway_auth_override.clone();
                        let empty_diagnosis = empty_diagnosis();
                        let empty_config = empty_config_excerpt_context();
                        async move {
                            ensure_agent_bridge_connected(
                                &app_handle,
                                &bridge_client,
                                &gateway_url,
                                gateway_auth_override.as_deref(),
                                session_id,
                            )
                            .await;
                            let text = if bridge_client.is_connected().await {
                                run_agent_request_with_bridge(
                                    &app_handle,
                                    client,
                                    &bridge_client,
                                    pool_ref,
                                    target_location,
                                    instance_id,
                                    remote_doctor_agent_id(),
                                    &remote_doctor_agent_session_key(session_id),
                                    &build_agent_plan_prompt(
                                        kind,
                                        session_id,
                                        round,
                                        target_location,
                                        instance_id,
                                        &empty_diagnosis,
                                        &empty_config,
                                        &previous_results,
                                    ),
                                )
                                .await?
                            } else {
                                client
                                    .run_agent_request(
                                        remote_doctor_agent_id(),
                                        &remote_doctor_agent_session_key(session_id),
                                        &build_agent_plan_prompt(
                                            kind,
                                            session_id,
                                            round,
                                            target_location,
                                            instance_id,
                                            &empty_diagnosis,
                                            &empty_config,
                                            &previous_results,
                                        ),
                                    )
                                    .await?
                            };
                            parse_agent_plan_response(kind, &text)
                        }
                    },
                )
                .await;
                if matches!(&agent, Err(error) if is_unknown_method_error(error)) {
                    append_remote_doctor_log(
                        &session_id,
                        json!({
                            "event": "protocol_fallback",
                            "from": "agent",
                            "to": "legacy_doctor",
                            "reason": agent.as_ref().err(),
                        }),
                    );
                    run_remote_doctor_repair_loop(
                        Some(&app),
                        pool_ref,
                        &session_id,
                        &instance_id,
                        target_location,
                        |kind, round, previous_results| {
                            let method = match kind {
                                PlanKind::Detect => detect_method_name(),
                                PlanKind::Investigate => repair_method_name(),
                                PlanKind::Repair => repair_method_name(),
                            };
                            let client = &client;
                            let session_id = &session_id;
                            let instance_id = &instance_id;
                            async move {
                                request_plan(
                                    client,
                                    &method,
                                    kind,
                                    session_id,
                                    round,
                                    target_location,
                                    instance_id,
                                    &previous_results,
                                )
                                .await
                            }
                        },
                    )
                    .await
                } else {
                    agent
                }
            } else {
                clawpal_server
            }
        }
    };

    let _ = client.disconnect().await;
    let _ = bridge.disconnect().await;

    match result {
        Ok(done) => {
            append_remote_doctor_log(
                &session_id,
                json!({
                    "event": "session_complete",
                    "status": "completed",
                    "latestDiagnosisHealthy": done.latest_diagnosis_healthy,
                }),
            );
            Ok(done)
        }
        Err(error) => {
            append_remote_doctor_log(
                &session_id,
                json!({
                    "event": "session_complete",
                    "status": "failed",
                    "reason": error,
                }),
            );
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn start_remote_doctor_repair(
    app: AppHandle,
    pool: State<'_, SshConnectionPool>,
    instance_id: String,
    target_location: String,
) -> Result<RemoteDoctorRepairResult, String> {
    start_remote_doctor_repair_impl(app, &pool, instance_id, target_location).await
}

#[cfg(test)]
mod tests {
    use super::*;
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

        assert!(!repair_plan_stalled(
            &[
                RepairRoundObservation::new(1, &step_types, &diagnosis),
                RepairRoundObservation::new(2, &step_types, &diagnosis),
            ],
            3,
        ));
        assert!(repair_plan_stalled(
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
        let error = round_limit_error_message(&diagnosis, &["doctorRediagnose".to_string()]);
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
        let result = run_remote_doctor_repair_loop(
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
        let result = run_clawpal_server_repair_loop(
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

        let result = start_remote_doctor_repair_impl(
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

        let result = start_remote_doctor_repair_impl(
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
