use std::time::Instant;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Runtime};

use super::agent::{
    build_agent_plan_prompt, remote_doctor_agent_id, remote_doctor_agent_session_key,
};
use super::config::{
    append_diagnosis_log, build_gateway_credentials as remote_doctor_gateway_credentials,
    diagnosis_missing_rescue_profile, diagnosis_unhealthy_rescue_gateway,
    primary_remote_target_host_id, remote_target_host_id_candidates, run_rescue_diagnosis,
};
use super::plan::{execute_command, execute_invoke_payload, parse_plan_response};
use super::session::{
    append_session_log as append_remote_doctor_log, emit_session_progress as emit_progress,
};
use super::types::{CommandResult, ConfigExcerptContext, PlanKind, PlanResponse, TargetLocation};
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

pub(crate) struct RescueActivationFailure {
    pub(crate) message: String,
    pub(crate) activation_result: CommandResult,
    pub(crate) diagnostics: Vec<CommandResult>,
}

pub(crate) async fn ensure_rescue_profile_ready<R: Runtime>(
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

pub(crate) fn parse_agent_plan_response(
    kind: PlanKind,
    text: &str,
) -> Result<PlanResponse, String> {
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
}
