use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::shared::MAX_REMOTE_DOCTOR_ROUNDS;
use super::super::plan::execute_command;
use super::super::session::{append_session_log, emit_session_progress};
use super::super::types::{CommandResult, PlanKind, PlanResponse, RemoteDoctorRepairResult, TargetLocation};
use crate::ssh::SshConnectionPool;

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
