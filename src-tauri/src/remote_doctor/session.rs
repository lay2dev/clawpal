use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Runtime};

use super::types::{PlanKind, RemoteDoctorProgressEvent, RemoteDoctorRepairResult};
use crate::models::resolve_paths;

pub(crate) fn session_log_dir() -> PathBuf {
    resolve_paths().clawpal_dir.join("doctor").join("remote")
}

pub(crate) fn append_session_log(session_id: &str, payload: Value) {
    let dir = session_log_dir();
    if create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{session_id}.jsonl"));
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{}", payload);
}

pub(crate) fn emit_session_progress<R: Runtime>(
    app: Option<&AppHandle<R>>,
    session_id: &str,
    round: usize,
    phase: &str,
    line: impl Into<String>,
    plan_kind: Option<PlanKind>,
    command: Option<Vec<String>>,
) {
    let payload = progress_event(session_id, round, phase, line, plan_kind, command);
    if let Some(app) = app {
        let _ = app.emit("doctor:remote-repair-progress", payload);
    }
}

pub(crate) fn result_for_completion(
    session_id: &str,
    round: usize,
    last_plan_kind: PlanKind,
    last_command: Option<Vec<String>>,
    message: &str,
) -> RemoteDoctorRepairResult {
    RemoteDoctorRepairResult {
        mode: "remoteDoctor".into(),
        status: "completed".into(),
        round,
        phase: "completed".into(),
        last_plan_kind: plan_kind_name(last_plan_kind).into(),
        latest_diagnosis_healthy: true,
        last_command,
        session_id: session_id.to_string(),
        message: message.into(),
    }
}

pub(crate) fn result_for_completion_with_warnings(
    session_id: &str,
    round: usize,
    last_plan_kind: PlanKind,
    last_command: Option<Vec<String>>,
    message: &str,
) -> RemoteDoctorRepairResult {
    RemoteDoctorRepairResult {
        mode: "remoteDoctor".into(),
        status: "completed_with_warnings".into(),
        round,
        phase: "completed".into(),
        last_plan_kind: plan_kind_name(last_plan_kind).into(),
        latest_diagnosis_healthy: false,
        last_command,
        session_id: session_id.to_string(),
        message: message.into(),
    }
}

fn progress_event(
    session_id: &str,
    round: usize,
    phase: &str,
    line: impl Into<String>,
    plan_kind: Option<PlanKind>,
    command: Option<Vec<String>>,
) -> RemoteDoctorProgressEvent {
    RemoteDoctorProgressEvent {
        session_id: session_id.to_string(),
        mode: "remoteDoctor".into(),
        round,
        phase: phase.to_string(),
        line: line.into(),
        plan_kind: plan_kind.map(|kind| plan_kind_name(kind).into()),
        command,
    }
}

fn plan_kind_name(kind: PlanKind) -> &'static str {
    match kind {
        PlanKind::Detect => "detect",
        PlanKind::Investigate => "investigate",
        PlanKind::Repair => "repair",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::cli_runner::set_active_clawpal_data_override;

    #[test]
    fn append_session_log_writes_jsonl_line() {
        let temp_root = std::env::temp_dir().join("clawpal-remote-doctor-session-log-test");
        let clawpal_dir = temp_root.join(".clawpal");
        std::fs::create_dir_all(&clawpal_dir).expect("create clawpal dir");
        set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
            .expect("set clawpal override");

        append_session_log("sess-1", json!({"event": "hello"}));

        set_active_clawpal_data_override(None).expect("clear clawpal override");

        let log_path = clawpal_dir
            .join("doctor")
            .join("remote")
            .join("sess-1.jsonl");
        let log_text = std::fs::read_to_string(&log_path).expect("read session log");
        assert!(log_text.contains("\"event\":\"hello\""));

        let _ = std::fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn progress_event_uses_snake_case_plan_kind() {
        let payload = progress_event(
            "sess-1",
            2,
            "planning_repair",
            "Requesting repair plan",
            Some(PlanKind::Repair),
            None,
        );
        assert_eq!(payload.plan_kind.as_deref(), Some("repair"));
    }

    #[test]
    fn completion_helpers_preserve_session_round_and_last_command() {
        let last_command = Some(vec!["clawpal".to_string(), "doctor".to_string()]);

        let completed =
            result_for_completion("sess-1", 4, PlanKind::Detect, last_command.clone(), "done");
        assert_eq!(completed.session_id, "sess-1");
        assert_eq!(completed.round, 4);
        assert_eq!(completed.last_command, last_command);
        assert!(completed.latest_diagnosis_healthy);

        let warning =
            result_for_completion_with_warnings("sess-2", 5, PlanKind::Repair, None, "warning");
        assert_eq!(warning.session_id, "sess-2");
        assert_eq!(warning.round, 5);
        assert_eq!(warning.last_plan_kind, "repair");
        assert!(!warning.latest_diagnosis_healthy);
    }
}
