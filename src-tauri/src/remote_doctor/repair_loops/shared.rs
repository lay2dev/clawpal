use serde_json::Value;

use super::super::types::{diagnosis_issue_summaries, ClawpalServerPlanStep, RepairRoundObservation};

pub(crate) const MAX_REMOTE_DOCTOR_ROUNDS: usize = 50;
pub(crate) const REPAIR_PLAN_STALL_THRESHOLD: usize = 3;

pub(crate) fn is_unknown_method_error(error: &str) -> bool {
    error.contains("unknown method")
        || error.contains("\"code\":\"INVALID_REQUEST\"")
        || error.contains("\"code\": \"INVALID_REQUEST\"")
}

pub(crate) fn clawpal_server_step_type_summary(steps: &[ClawpalServerPlanStep]) -> Value {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{RescuePrimaryDiagnosisResult, RescuePrimaryIssue, RescuePrimarySummary};
    use crate::remote_doctor::types::RepairRoundObservation;

    #[test]
    fn repeated_rediagnose_only_rounds_are_detected_as_stalled() {
        let diagnosis = sample_diagnosis(vec![RescuePrimaryIssue {
            id: "issue-1".to_string(),
            code: "invalid.base_url".to_string(),
            severity: "medium".to_string(),
            message: "Provider base URL is invalid".to_string(),
            auto_fixable: true,
            fix_hint: Some("Reset baseUrl".to_string()),
            source: "config".to_string(),
        }]);
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
        let diagnosis = sample_diagnosis(vec![RescuePrimaryIssue {
            id: "issue-1".to_string(),
            code: "invalid.base_url".to_string(),
            severity: "medium".to_string(),
            message: "Provider base URL is invalid".to_string(),
            auto_fixable: true,
            fix_hint: Some("Reset baseUrl".to_string()),
            source: "config".to_string(),
        }]);
        let error = round_limit_error_message(&diagnosis, &["doctorRediagnose".to_string()]);
        assert!(error.contains("invalid.base_url"));
        assert!(error.contains("doctorRediagnose"));
        assert!(error.contains("Provider base URL is invalid"));
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
