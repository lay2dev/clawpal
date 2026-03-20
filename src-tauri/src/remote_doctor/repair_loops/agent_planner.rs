use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::super::agent::next_agent_plan_kind_for_round;
use super::super::config::{
    append_diagnosis_log, build_config_excerpt_context, config_excerpt_log_summary,
    diagnosis_has_only_non_auto_fixable_issues, diagnosis_is_healthy, read_target_config_raw,
    run_rescue_diagnosis,
};
use super::super::legacy::request_agent_plan;
use super::super::plan::{
    agent_plan_step_types, execute_plan_command, plan_command_failure_message,
};
use super::super::session::{
    append_session_log, emit_session_progress, result_for_completion,
    result_for_completion_with_warnings,
};
use super::super::types::{
    diagnosis_issue_summaries, CommandResult, PlanKind, RemoteDoctorRepairResult,
    RepairRoundObservation, TargetLocation,
};
use super::shared::{
    repair_plan_stalled, round_limit_error_message, stalled_plan_error_message,
    MAX_REMOTE_DOCTOR_ROUNDS, REPAIR_PLAN_STALL_THRESHOLD,
};
use crate::bridge_client::BridgeClient;
use crate::node_client::NodeClient;
use crate::ssh::SshConnectionPool;

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
