use serde_json::{json, Value};
use tauri::{AppHandle, Runtime, State};
use uuid::Uuid;

use super::agent::{
    build_agent_plan_prompt, configured_remote_doctor_protocol, default_remote_doctor_protocol,
    detect_method_name, ensure_agent_workspace_ready, gateway_url_is_local,
    next_agent_plan_kind_for_round, protocol_requires_bridge, protocol_runs_rescue_preflight,
    remote_doctor_agent_id, remote_doctor_agent_session_key, repair_method_name,
};
use super::config::{
    append_diagnosis_log, build_gateway_credentials, config_excerpt_log_summary,
    diagnosis_has_only_non_auto_fixable_issues, diagnosis_is_healthy,
    empty_config_excerpt_context, empty_diagnosis, load_gateway_config, read_target_config_raw,
    run_rescue_diagnosis,
};
use super::legacy::{
    ensure_agent_bridge_connected, ensure_remote_target_connected, parse_agent_plan_response,
    repair_rescue_gateway_if_needed, request_agent_plan, run_agent_request_with_bridge,
};
use super::plan::{
    agent_plan_step_types, apply_config_set, apply_config_unset, execute_command,
    execute_plan_command, plan_command_failure_message,
    report_clawpal_server_final_result, report_clawpal_server_step_result,
    request_clawpal_server_plan, request_plan,
};
use super::session::{
    append_session_log, emit_session_progress, result_for_completion,
    result_for_completion_with_warnings,
};
use super::types::{
    diagnosis_issue_summaries, parse_target_location, ClawpalServerPlanStep, CommandResult,
    PlanKind, PlanResponse, RemoteDoctorProtocol, RemoteDoctorRepairResult, RepairRoundObservation,
    TargetLocation,
};
use crate::bridge_client::BridgeClient;
use crate::commands::logs::log_dev;
use crate::node_client::NodeClient;
use crate::ssh::SshConnectionPool;

const MAX_REMOTE_DOCTOR_ROUNDS: usize = 50;
const REPAIR_PLAN_STALL_THRESHOLD: usize = 3;

fn is_unknown_method_error(error: &str) -> bool {
    error.contains("unknown method")
        || error.contains("\"code\":\"INVALID_REQUEST\"")
        || error.contains("\"code\": \"INVALID_REQUEST\"")
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

pub(crate) fn repair_plan_stalled(
    observations: &[RepairRoundObservation],
    threshold: usize,
) -> bool {
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

pub(crate) fn round_limit_error_message(
    diagnosis: &crate::commands::RescuePrimaryDiagnosisResult,
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

pub(crate) fn stalled_plan_error_message(observation: &RepairRoundObservation) -> String {
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

pub(crate) async fn run_remote_doctor_repair_loop<R: Runtime, F, Fut>(
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
        emit_session_progress(
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
        append_session_log(
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
            emit_session_progress(
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
            append_session_log(
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

        emit_session_progress(
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
        append_session_log(
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
            emit_session_progress(
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
            append_session_log(
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

    append_session_log(
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

pub(crate) async fn run_clawpal_server_repair_loop<R: Runtime>(
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
        emit_session_progress(
            Some(app),
            session_id,
            round,
            "planning_repair",
            format!("Requesting remote repair plan for round {round}"),
            Some(PlanKind::Repair),
            None,
        );
        let config_context = super::config::build_config_excerpt_context(
            &read_target_config_raw(app, target_location, instance_id).await?,
        );
        append_session_log(
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
            append_session_log(
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
        append_session_log(
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
            let started = std::time::Instant::now();
            match step.step_type.as_str() {
                "configSet" => {
                    let path = step.path.as_deref().ok_or("configSet step missing path")?;
                    let value = step.value.clone().ok_or("configSet step missing value")?;
                    emit_session_progress(
                        Some(app),
                        session_id,
                        round,
                        "executing_repair",
                        format!("Applying config set: {path}"),
                        Some(PlanKind::Repair),
                        None,
                    );
                    apply_config_set(&mut current_config, path, value)?;
                    super::config::write_target_config(app, target_location, instance_id, &current_config).await?;
                    super::config::restart_target_gateway(app, target_location, instance_id).await?;
                    result.argv = vec!["configSet".into(), path.into()];
                    result.stdout = format!("Updated {path}");
                }
                "configUnset" => {
                    let path = step.path.as_deref().ok_or("configUnset step missing path")?;
                    emit_session_progress(
                        Some(app),
                        session_id,
                        round,
                        "executing_repair",
                        format!("Applying config unset: {path}"),
                        Some(PlanKind::Repair),
                        None,
                    );
                    apply_config_unset(&mut current_config, path)?;
                    super::config::write_target_config(app, target_location, instance_id, &current_config).await?;
                    super::config::restart_target_gateway(app, target_location, instance_id).await?;
                    result.argv = vec!["configUnset".into(), path.into()];
                    result.stdout = format!("Removed {path}");
                }
                "doctorRediagnose" => {
                    emit_session_progress(
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
            append_session_log(
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
            append_session_log(
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

pub(crate) async fn run_agent_planner_repair_loop<R: Runtime>(
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
        let config_context = super::config::build_config_excerpt_context(
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
        emit_session_progress(Some(app), session_id, round, phase, line, Some(kind), None);
        append_session_log(
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
        append_session_log(
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
            emit_session_progress(
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
            append_session_log(
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
            append_session_log(
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
            append_session_log(
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

pub(crate) async fn start_remote_doctor_repair_impl<R: Runtime>(
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
    let gateway = load_gateway_config()?;
    let creds = build_gateway_credentials(gateway.auth_token_override.as_deref())?;
    log_dev(format!(
        "[remote_doctor] start session={} instance_id={} target_location={:?} gateway_url={} auth_token_override={}",
        session_id,
        instance_id,
        target_location,
        gateway.url,
        gateway.auth_token_override.is_some()
    ));
    append_session_log(
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
        ensure_agent_workspace_ready()?;
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
                append_session_log(
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
                append_session_log(
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
                append_session_log(
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
                    append_session_log(
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
            append_session_log(
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
            append_session_log(
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
