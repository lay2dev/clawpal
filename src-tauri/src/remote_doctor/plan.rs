use std::time::Instant;

use base64::Engine;
use serde_json::{json, Value};
use tauri::{AppHandle, Runtime};
use uuid::Uuid;

use super::config::{
    build_config_excerpt_context, diagnosis_context, primary_remote_target_host_id,
    read_target_config, read_target_config_raw, restart_target_gateway, write_target_config,
    write_target_config_raw,
};
use super::types::{
    ClawpalServerPlanResponse, ClawpalServerPlanStep, CommandResult, ConfigExcerptContext,
    PlanKind, PlanResponse, TargetLocation,
};
use crate::cli_runner::{get_active_openclaw_home_override, run_openclaw, run_openclaw_remote};
use crate::commands::RescuePrimaryDiagnosisResult;
use crate::node_client::NodeClient;
use crate::ssh::SshConnectionPool;

pub(crate) fn parse_invoke_argv(command: &str, args: &Value) -> Result<Vec<String>, String> {
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

pub(crate) async fn execute_clawpal_command<R: Runtime>(
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

pub(crate) async fn execute_clawpal_doctor_command<R: Runtime>(
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
            let which_result = execute_command(
                pool,
                target_location,
                instance_id,
                &["sh".into(), "-lc".into(), "command -v openclaw || true".into()],
            )
            .await?;
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
            Ok(json!({ "raw": raw }))
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
            Ok(json!({ "written": true, "bytes": decoded.len() }))
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

pub(crate) fn config_read_response(raw: &str, path: Option<&str>) -> Result<Value, String> {
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

pub(crate) fn decode_base64_config_payload(encoded: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|error| format!("Failed to decode base64 config payload: {error}"))?;
    String::from_utf8(bytes)
        .map_err(|error| format!("Base64 config payload is not valid UTF-8: {error}"))
}

pub(crate) async fn execute_invoke_payload<R: Runtime>(
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

pub(crate) fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn build_shell_command(argv: &[String]) -> String {
    argv.iter()
        .map(|part| shell_escape(part))
        .collect::<Vec<String>>()
        .join(" ")
}

pub(crate) async fn execute_command(
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
                let arg_refs = argv.iter().skip(1).map(String::as_str).collect::<Vec<&str>>();
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
                let arg_refs = argv.iter().skip(1).map(String::as_str).collect::<Vec<&str>>();
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
                let output = pool.exec_login(&host_id, &build_shell_command(argv)).await?;
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

pub(crate) fn plan_command_uses_internal_clawpal_tool(argv: &[String]) -> bool {
    argv.first().map(String::as_str) == Some("clawpal")
}

pub(crate) fn validate_clawpal_exec_args(argv: &[String]) -> Result<(), String> {
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

pub(crate) fn validate_plan_command_argv(argv: &[String]) -> Result<(), String> {
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
        Err(format!("Unsupported openclaw plan command: {}", argv.join(" ")))
    }
}

pub(crate) fn plan_command_failure_message(
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

pub(crate) fn command_result_stdout(value: &Value) -> String {
    value
        .get("stdout")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()))
}

pub(crate) async fn execute_plan_command<R: Runtime>(
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

pub(crate) fn parse_plan_response(kind: PlanKind, value: Value) -> Result<PlanResponse, String> {
    let mut response: PlanResponse = serde_json::from_value(value)
        .map_err(|error| format!("Failed to parse remote doctor plan response: {error}"))?;
    response.plan_kind = kind;
    if response.plan_id.trim().is_empty() {
        response.plan_id = format!("plan-{}", Uuid::new_v4());
    }
    Ok(response)
}

pub(crate) async fn request_plan(
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

pub(crate) fn agent_plan_step_types(plan: &PlanResponse) -> Vec<String> {
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

pub(crate) async fn request_clawpal_server_plan(
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

pub(crate) async fn report_clawpal_server_step_result(
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

pub(crate) async fn report_clawpal_server_final_result(
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

fn ensure_object(value: &mut Value) -> Result<&mut serde_json::Map<String, Value>, String> {
    if !value.is_object() {
        *value = json!({});
    }
    value
        .as_object_mut()
        .ok_or_else(|| "Expected object while applying remote doctor config step".to_string())
}

pub(crate) fn apply_config_set(root: &mut Value, path: &str, value: Value) -> Result<(), String> {
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

pub(crate) fn apply_config_unset(root: &mut Value, path: &str) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_shell_command_escapes_single_quotes() {
        let command = build_shell_command(&["echo".into(), "a'b".into()]);
        assert_eq!(command, "'echo' 'a'\\''b'");
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

    #[test]
    fn parse_plan_response_generates_plan_id_when_missing() {
        let plan = parse_plan_response(
            PlanKind::Detect,
            json!({
                "planId": "",
                "planKind": "detect",
                "summary": "ok",
                "commands": []
            }),
        )
        .expect("parse plan");
        assert!(!plan.plan_id.is_empty());
        assert_eq!(plan.plan_kind, PlanKind::Detect);
    }
}
