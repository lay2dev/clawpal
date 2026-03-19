use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::shared::{
    clawpal_server_step_type_summary, repair_plan_stalled, round_limit_error_message,
    stalled_plan_error_message, MAX_REMOTE_DOCTOR_ROUNDS, REPAIR_PLAN_STALL_THRESHOLD,
};
use super::super::config::{
    append_diagnosis_log, build_config_excerpt_context, config_excerpt_log_summary,
    diagnosis_is_healthy, read_target_config_raw, restart_target_gateway, run_rescue_diagnosis,
    write_target_config,
};
use super::super::legacy::repair_rescue_gateway_if_needed;
use super::super::plan::{
    apply_config_set, apply_config_unset, report_clawpal_server_final_result,
    report_clawpal_server_step_result, request_clawpal_server_plan,
};
use super::super::session::{append_session_log, emit_session_progress, result_for_completion};
use super::super::types::{
    diagnosis_issue_summaries, CommandResult, PlanKind, RemoteDoctorProtocol,
    RemoteDoctorRepairResult, RepairRoundObservation, TargetLocation,
};
use crate::node_client::NodeClient;

pub(crate) async fn run_clawpal_server_repair_loop<R: Runtime>(
    app: &AppHandle<R>,
    client: &NodeClient,
    session_id: &str,
    instance_id: &str,
    target_location: TargetLocation,
) -> Result<RemoteDoctorRepairResult, String> {
    let mut diagnosis = run_rescue_diagnosis(app, target_location, instance_id).await?;
    append_diagnosis_log(session_id, "initial", 0, &diagnosis);
    if super::super::agent::protocol_runs_rescue_preflight(RemoteDoctorProtocol::ClawpalServer) {
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
        let config_context =
            build_config_excerpt_context(&read_target_config_raw(app, target_location, instance_id).await?);
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
                    write_target_config(app, target_location, instance_id, &current_config).await?;
                    restart_target_gateway(app, target_location, instance_id).await?;
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
                    write_target_config(app, target_location, instance_id, &current_config).await?;
                    restart_target_gateway(app, target_location, instance_id).await?;
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
                    result.stdout =
                        format!("Diagnosis status={} issues={}", diagnosis.status, diagnosis.issues.len());
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
        if super::super::agent::protocol_runs_rescue_preflight(RemoteDoctorProtocol::ClawpalServer) {
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
        round_observations.push(RepairRoundObservation::new(round, &round_step_types, &diagnosis));
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
