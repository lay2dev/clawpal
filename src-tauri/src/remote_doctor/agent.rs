use std::fs::create_dir_all;
use std::path::PathBuf;

use serde_json::json;

use super::config::diagnosis_context;
use super::types::{
    CommandResult, ConfigExcerptContext, PlanKind, RemoteDoctorProtocol, TargetLocation,
};
use crate::commands::{
    agent::create_agent, agent::setup_agent_identity, RescuePrimaryDiagnosisResult,
};
use crate::config_io::read_openclaw_config;
use crate::models::resolve_paths;

const DEFAULT_DETECT_METHOD: &str = "doctor.get_detection_plan";
const DEFAULT_REPAIR_METHOD: &str = "doctor.get_repair_plan";
const REMOTE_DOCTOR_AGENT_ID: &str = "clawpal-remote-doctor";

pub(crate) fn detect_method_name() -> String {
    std::env::var("CLAWPAL_REMOTE_DOCTOR_DETECT_METHOD")
        .unwrap_or_else(|_| DEFAULT_DETECT_METHOD.to_string())
}

pub(crate) fn repair_method_name() -> String {
    std::env::var("CLAWPAL_REMOTE_DOCTOR_REPAIR_METHOD")
        .unwrap_or_else(|_| DEFAULT_REPAIR_METHOD.to_string())
}

pub(crate) fn configured_remote_doctor_protocol() -> Option<RemoteDoctorProtocol> {
    match std::env::var("CLAWPAL_REMOTE_DOCTOR_PROTOCOL")
        .ok()
        .as_deref()
        .map(str::trim)
    {
        Some("agent") => Some(RemoteDoctorProtocol::AgentPlanner),
        Some("legacy") | Some("legacy_doctor") => Some(RemoteDoctorProtocol::LegacyDoctor),
        Some("clawpal_server") => Some(RemoteDoctorProtocol::ClawpalServer),
        _ => None,
    }
}

pub(crate) fn default_remote_doctor_protocol() -> RemoteDoctorProtocol {
    RemoteDoctorProtocol::AgentPlanner
}

pub(crate) fn protocol_requires_bridge(protocol: RemoteDoctorProtocol) -> bool {
    matches!(protocol, RemoteDoctorProtocol::AgentPlanner)
}

pub(crate) fn protocol_runs_rescue_preflight(protocol: RemoteDoctorProtocol) -> bool {
    matches!(protocol, RemoteDoctorProtocol::LegacyDoctor)
}

pub(crate) fn next_agent_plan_kind(diagnosis: &RescuePrimaryDiagnosisResult) -> PlanKind {
    next_agent_plan_kind_for_round(diagnosis, &[])
}

pub(crate) fn next_agent_plan_kind_for_round(
    diagnosis: &RescuePrimaryDiagnosisResult,
    previous_results: &[CommandResult],
) -> PlanKind {
    if diagnosis
        .issues
        .iter()
        .any(|issue| issue.code == "primary.config.unreadable")
    {
        if !previous_results.is_empty() {
            return PlanKind::Repair;
        }
        PlanKind::Investigate
    } else {
        PlanKind::Repair
    }
}

pub(crate) fn remote_doctor_agent_id() -> &'static str {
    REMOTE_DOCTOR_AGENT_ID
}

pub(crate) fn remote_doctor_agent_session_key(session_id: &str) -> String {
    format!("agent:{}:{session_id}", remote_doctor_agent_id())
}

fn agent_workspace_bootstrap_files() -> [(&'static str, &'static str); 4] {
    [
        (
            "AGENTS.md",
            "# Remote Doctor\nUse this workspace only for ClawPal remote doctor planning sessions.\nReturn structured, operational answers.\n",
        ),
        (
            "BOOTSTRAP.md",
            "Bootstrap is already complete for this workspace.\nDo not ask who you are or who the user is.\nUse IDENTITY.md and USER.md as the canonical identity context.\n",
        ),
        (
            "USER.md",
            "- Name: ClawPal Desktop\n- Role: desktop repair orchestrator\n- Preferences: concise, operational, no bootstrap chatter\n",
        ),
        (
            "HEARTBEAT.md",
            "Status: active remote-doctor planning workspace.\n",
        ),
    ]
}

pub(crate) fn gateway_url_is_local(url: &str) -> bool {
    let rest = url
        .split_once("://")
        .map(|(_, remainder)| remainder)
        .unwrap_or(url);
    let host_port = rest.split('/').next().unwrap_or(rest);
    let host = host_port
        .strip_prefix('[')
        .and_then(|value| value.split_once(']').map(|(host, _)| host))
        .unwrap_or_else(|| host_port.split(':').next().unwrap_or(host_port));
    matches!(host, "127.0.0.1" | "localhost")
}

pub(crate) fn ensure_agent_workspace_ready() -> Result<(), String> {
    let agent_id = remote_doctor_agent_id().to_string();
    if let Err(error) = create_agent(agent_id.clone(), None, Some(true)) {
        if !error.contains("already exists") {
            return Err(format!("Failed to create remote doctor agent: {error}"));
        }
    }

    setup_agent_identity(agent_id.clone(), "ClawPal Remote Doctor".to_string(), None)?;

    let paths = resolve_paths();
    let cfg = read_openclaw_config(&paths)?;
    let workspace =
        clawpal_core::doctor::resolve_agent_workspace_from_config(&cfg, &agent_id, None)
            .map(|path| shellexpand::tilde(&path).to_string())?;
    create_dir_all(&workspace)
        .map_err(|error| format!("Failed to create remote doctor workspace: {error}"))?;

    for (file_name, content) in agent_workspace_bootstrap_files() {
        std::fs::write(PathBuf::from(&workspace).join(file_name), content)
            .map_err(|error| format!("Failed to write remote doctor {file_name}: {error}"))?;
    }

    Ok(())
}

pub(crate) fn build_agent_plan_prompt(
    kind: PlanKind,
    session_id: &str,
    round: usize,
    target_location: TargetLocation,
    instance_id: &str,
    diagnosis: &RescuePrimaryDiagnosisResult,
    config_context: &ConfigExcerptContext,
    previous_results: &[CommandResult],
) -> String {
    let kind_label = match kind {
        PlanKind::Detect => "detection",
        PlanKind::Investigate => "investigation",
        PlanKind::Repair => "repair",
    };
    let target_label = match target_location {
        TargetLocation::LocalOpenclaw => "local_openclaw",
        TargetLocation::RemoteOpenclaw => "remote_openclaw",
    };
    let diagnosis_json =
        serde_json::to_string_pretty(&diagnosis_context(diagnosis)).unwrap_or_else(|_| "{}".into());
    let config_context_json = serde_json::to_string_pretty(&json!({
        "configExcerpt": config_context.config_excerpt,
        "configExcerptRaw": config_context.config_excerpt_raw,
        "configParseError": config_context.config_parse_error,
    }))
    .unwrap_or_else(|_| "{}".into());
    let previous_results_json =
        serde_json::to_string_pretty(previous_results).unwrap_or_else(|_| "[]".into());
    let phase_rules = match kind {
        PlanKind::Detect => "For detection plans, gather only the commands needed to confirm current state. Set healthy=true and done=true only when no issue remains.",
        PlanKind::Investigate => "For investigation plans, return read-only diagnosis steps only. Do not modify files, delete files, overwrite config, or restart services. Prefer commands that inspect, validate, backup, or print evidence for why the config is unreadable. Do not run follow/tail commands, streaming log readers, or any unbounded command; every investigation command must be bounded and return promptly. Do not use heredocs, multiline scripts, or commands that wait on stdin. Prefer single-line commands over shell scripting.",
        PlanKind::Repair => "For repair plans, return the minimal safe repair commands. Reference prior investigation evidence when config is unreadable. Back up the file before changing it and include validation/rediagnosis steps as needed. Do not invent OpenClaw subcommands. Use only the verified OpenClaw commands listed below or the `clawpal doctor ...` tools. Do not use `openclaw auth ...` commands. Do not use `openclaw doctor --json`; use `clawpal doctor probe-openclaw` or `clawpal doctor exec --tool doctor` instead. Do not use heredocs, multiline scripts, or commands that wait on stdin.",
    };
    format!(
        "Identity bootstrap for this session:\n\
- Your name: ClawPal Remote Doctor\n\
- Your creature: maintenance daemon\n\
- Your vibe: direct, terse, operational\n\
- Your emoji: none\n\
- The user is: ClawPal desktop app\n\
- The user timezone is: Asia/Shanghai\n\
- Do not ask identity/bootstrap questions.\n\
- Do not ask who you are or who the user is.\n\
- Do not modify IDENTITY.md, USER.md, or workspace bootstrap files.\n\
\n\
You are ClawPal Remote Doctor planner.\n\
Return ONLY one JSON object and no markdown.\n\
Task: produce the next {kind_label} plan for OpenClaw.\n\
Session: {session_id}\n\
Round: {round}\n\
Target location: {target_label}\n\
Instance id: {instance_id}\n\
Diagnosis JSON:\n{diagnosis_json}\n\n\
Config context JSON:\n{config_context_json}\n\n\
Previous command results JSON:\n{previous_results_json}\n\n\
Available gateway tools:\n\
- `clawpal doctor probe-openclaw`\n\
- `clawpal doctor config-read [path]`\n\
- `clawpal doctor config-read-raw`\n\
- `clawpal doctor config-upsert <path> <json>`\n\
- `clawpal doctor config-delete <path>`\n\
- `clawpal doctor config-write-raw-base64 <base64-utf8-json>`\n\
- `clawpal doctor exec --tool <command> [--args <shell-escaped-args>]`\n\
- Verified direct OpenClaw commands only:\n\
  - `openclaw --version`\n\
  - `openclaw gateway status`\n\
You may invoke these tools before answering when you need fresh diagnostics or config state.\n\
If you already have enough information, return the JSON plan directly.\n\n\
Return this exact JSON schema:\n\
{{\n  \"planId\": \"string\",\n  \"planKind\": \"{kind}\",\n  \"summary\": \"string\",\n  \"commands\": [{{\"argv\": [\"cmd\"], \"timeoutSec\": 60, \"purpose\": \"why\", \"continueOnFailure\": false}}],\n  \"healthy\": false,\n  \"done\": false,\n  \"success\": false\n}}\n\
Rules:\n\
- {phase_rules}\n\
- For repair plans, return shell/openclaw commands in commands.\n\
- Keep commands empty when no command is needed.\n\
- Output valid JSON only.",
        kind = match kind {
            PlanKind::Detect => "detect",
            PlanKind::Investigate => "investigate",
            PlanKind::Repair => "repair",
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_runner::{set_active_clawpal_data_override, set_active_openclaw_home_override};
    use crate::commands::{RescuePrimaryDiagnosisResult, RescuePrimaryIssue, RescuePrimarySummary};

    #[test]
    fn default_remote_doctor_protocol_prefers_agent() {
        assert_eq!(
            default_remote_doctor_protocol(),
            RemoteDoctorProtocol::AgentPlanner
        );
    }

    #[test]
    fn unreadable_config_switches_to_repair_after_investigation_results_exist() {
        let diagnosis = sample_diagnosis(vec![RescuePrimaryIssue {
            id: "issue-1".into(),
            code: "primary.config.unreadable".into(),
            severity: "error".into(),
            message: "Primary configuration could not be read".into(),
            auto_fixable: false,
            fix_hint: Some("Repair".into()),
            source: "primary".into(),
        }]);
        let previous_results = vec![CommandResult {
            argv: vec!["clawpal".into(), "doctor".into(), "config-read-raw".into()],
            exit_code: Some(0),
            stdout: "{}".into(),
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
    }

    #[test]
    fn unreadable_config_requires_investigate_plan_kind() {
        let diagnosis = sample_diagnosis(vec![RescuePrimaryIssue {
            id: "issue-1".into(),
            code: "primary.config.unreadable".into(),
            severity: "error".into(),
            message: "Primary configuration could not be read".into(),
            auto_fixable: false,
            fix_hint: Some("Repair".into()),
            source: "primary".into(),
        }]);
        assert_eq!(next_agent_plan_kind(&diagnosis), PlanKind::Investigate);
    }

    #[test]
    fn investigate_prompt_requires_read_only_diagnosis_steps() {
        let diagnosis = sample_diagnosis(vec![RescuePrimaryIssue {
            id: "issue-1".into(),
            code: "primary.config.unreadable".into(),
            severity: "error".into(),
            message: "Primary configuration could not be read".into(),
            auto_fixable: false,
            fix_hint: Some("Repair".into()),
            source: "primary".into(),
        }]);
        let prompt = build_agent_plan_prompt(
            PlanKind::Investigate,
            "sess-1",
            1,
            TargetLocation::RemoteOpenclaw,
            "ssh:vm1",
            &diagnosis,
            &ConfigExcerptContext {
                config_excerpt: serde_json::Value::Null,
                config_excerpt_raw: Some("{\n  ddd\n}".into()),
                config_parse_error: Some(
                    "Failed to parse target config: key must be a string".into(),
                ),
            },
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
            &ConfigExcerptContext {
                config_excerpt: serde_json::Value::Null,
                config_excerpt_raw: None,
                config_parse_error: None,
            },
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
            &ConfigExcerptContext {
                config_excerpt: serde_json::Value::Null,
                config_excerpt_raw: None,
                config_parse_error: None,
            },
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
    fn ensure_agent_workspace_ready_creates_workspace_bootstrap_files() {
        let temp_root = std::env::temp_dir().join(format!(
            "clawpal-remote-doctor-agent-test-{}",
            uuid::Uuid::new_v4()
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

        let result = ensure_agent_workspace_ready();

        set_active_openclaw_home_override(None).expect("clear openclaw override");
        set_active_clawpal_data_override(None).expect("clear clawpal override");
        if let Err(error) = &result {
            let _ = std::fs::remove_dir_all(&temp_root);
            panic!("ensure agent ready: {error}");
        }
        let cfg: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(openclaw_dir.join("openclaw.json")).expect("read config"),
        )
        .expect("parse config");
        let agent = cfg["agents"]["list"]
            .as_array()
            .and_then(|agents| {
                agents.iter().find(|agent| {
                    agent.get("id").and_then(serde_json::Value::as_str)
                        == Some(remote_doctor_agent_id())
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
