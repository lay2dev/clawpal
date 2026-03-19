use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::commands::{RescuePrimaryDiagnosisResult, RescuePrimaryIssue};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TargetLocation {
    LocalOpenclaw,
    RemoteOpenclaw,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlanKind {
    Detect,
    Investigate,
    Repair,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlanCommand {
    pub(crate) argv: Vec<String>,
    pub(crate) timeout_sec: Option<u64>,
    pub(crate) purpose: Option<String>,
    pub(crate) continue_on_failure: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlanResponse {
    pub(crate) plan_id: String,
    pub(crate) plan_kind: PlanKind,
    pub(crate) summary: String,
    #[serde(default)]
    pub(crate) commands: Vec<PlanCommand>,
    #[serde(default)]
    pub(crate) healthy: bool,
    #[serde(default)]
    pub(crate) done: bool,
    #[serde(default)]
    pub(crate) success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommandResult {
    pub(crate) argv: Vec<String>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) duration_ms: u64,
    pub(crate) timed_out: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteDoctorProtocol {
    AgentPlanner,
    LegacyDoctor,
    ClawpalServer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClawpalServerPlanResponse {
    pub(crate) request_id: String,
    pub(crate) plan_id: String,
    pub(crate) summary: String,
    #[serde(default)]
    pub(crate) steps: Vec<ClawpalServerPlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClawpalServerPlanStep {
    #[serde(rename = "type")]
    pub(crate) step_type: String,
    pub(crate) path: Option<String>,
    pub(crate) value: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDoctorRepairResult {
    pub(crate) mode: String,
    pub(crate) status: String,
    pub(crate) round: usize,
    pub(crate) phase: String,
    pub(crate) last_plan_kind: String,
    pub(crate) latest_diagnosis_healthy: bool,
    pub(crate) last_command: Option<Vec<String>>,
    pub(crate) session_id: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteDoctorProgressEvent {
    pub(crate) session_id: String,
    pub(crate) mode: String,
    pub(crate) round: usize,
    pub(crate) phase: String,
    pub(crate) line: String,
    pub(crate) plan_kind: Option<String>,
    pub(crate) command: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub(crate) struct ConfigExcerptContext {
    pub(crate) config_excerpt: Value,
    pub(crate) config_excerpt_raw: Option<String>,
    pub(crate) config_parse_error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct RepairRoundObservation {
    pub(crate) round: usize,
    pub(crate) step_types: Vec<String>,
    pub(crate) diagnosis_signature: String,
    pub(crate) issue_summaries: Vec<Value>,
}

impl RepairRoundObservation {
    pub(crate) fn new(
        round: usize,
        step_types: &[String],
        diagnosis: &RescuePrimaryDiagnosisResult,
    ) -> Self {
        let issue_summaries = diagnosis_issue_summaries(diagnosis);
        let diagnosis_signature =
            serde_json::to_string(&issue_summaries).unwrap_or_else(|_| "[]".to_string());
        Self {
            round,
            step_types: step_types.to_vec(),
            diagnosis_signature,
            issue_summaries,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredRemoteDoctorIdentity {
    pub(crate) version: u8,
    pub(crate) created_at_ms: u64,
    pub(crate) device_id: String,
    pub(crate) private_key_pem: String,
}

pub(crate) fn parse_target_location(raw: &str) -> Result<TargetLocation, String> {
    match raw {
        "local_openclaw" => Ok(TargetLocation::LocalOpenclaw),
        "remote_openclaw" => Ok(TargetLocation::RemoteOpenclaw),
        other => Err(format!("Unsupported target location: {other}")),
    }
}

pub(crate) fn diagnosis_issue_summaries(diagnosis: &RescuePrimaryDiagnosisResult) -> Vec<Value> {
    diagnosis.issues.iter().map(summarize_issue).collect()
}

fn summarize_issue(issue: &RescuePrimaryIssue) -> Value {
    json!({
        "id": issue.id,
        "code": issue.code,
        "severity": issue.severity,
        "title": issue.message,
        "target": issue.source,
        "autoFixable": issue.auto_fixable,
        "fixHint": issue.fix_hint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::RescuePrimarySummary;

    #[test]
    fn parse_target_location_accepts_known_values() {
        assert_eq!(
            parse_target_location("local_openclaw").unwrap(),
            TargetLocation::LocalOpenclaw
        );
        assert_eq!(
            parse_target_location("remote_openclaw").unwrap(),
            TargetLocation::RemoteOpenclaw
        );
    }

    #[test]
    fn parse_target_location_rejects_unknown_values() {
        let error = parse_target_location("elsewhere").unwrap_err();
        assert!(error.contains("Unsupported target location"));
    }

    #[test]
    fn repair_round_observation_uses_stable_diagnosis_signature() {
        let step_types = vec!["repair_config".to_string()];
        let diagnosis = sample_diagnosis(vec![RescuePrimaryIssue {
            id: "issue-1".to_string(),
            code: "primary.config.unreadable".to_string(),
            severity: "error".to_string(),
            message: "Unreadable config".to_string(),
            auto_fixable: false,
            fix_hint: Some("Repair the config".to_string()),
            source: "primary".to_string(),
        }]);

        let first = RepairRoundObservation::new(1, &step_types, &diagnosis);
        let second = RepairRoundObservation::new(2, &step_types, &diagnosis);

        assert_eq!(first.diagnosis_signature, second.diagnosis_signature);
        assert_eq!(first.issue_summaries, second.issue_summaries);
    }

    fn sample_diagnosis(issues: Vec<RescuePrimaryIssue>) -> RescuePrimaryDiagnosisResult {
        RescuePrimaryDiagnosisResult {
            status: "degraded".to_string(),
            checked_at: "2026-03-19T00:00:00Z".to_string(),
            target_profile: "primary".to_string(),
            rescue_profile: "rescue".to_string(),
            rescue_configured: true,
            rescue_port: Some(18789),
            summary: RescuePrimarySummary {
                status: "degraded".to_string(),
                headline: "Issues found".to_string(),
                recommended_action: "Repair".to_string(),
                fixable_issue_count: 0,
                selected_fix_issue_ids: Vec::new(),
                root_cause_hypotheses: Vec::new(),
                fix_steps: Vec::new(),
                confidence: None,
                citations: Vec::new(),
                version_awareness: None,
            },
            sections: Vec::new(),
            checks: Vec::new(),
            issues,
        }
    }
}
