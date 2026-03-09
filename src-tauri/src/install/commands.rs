use super::runners;
use super::session_store::InstallSessionStore;
use super::types::{
    InstallMethod, InstallMethodCapability, InstallSession, InstallState, InstallStep,
    InstallStepResult,
};
use crate::ssh::SshConnectionPool;
use chrono::Utc;
use clawpal_core::ssh::diagnostic::{
    from_any_error, SshDiagnosticReport, SshErrorCode, SshIntent, SshStage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;
use tauri::State;
use uuid::Uuid;

static TEST_SESSION_STORE: LazyLock<InstallSessionStore> = LazyLock::new(InstallSessionStore::new);

fn parse_method(raw: &str) -> Result<InstallMethod, String> {
    match raw {
        "local" => Ok(InstallMethod::Local),
        "wsl2" => Ok(InstallMethod::Wsl2),
        "docker" => Ok(InstallMethod::Docker),
        "remote_ssh" => Ok(InstallMethod::RemoteSsh),
        _ => Err(format!("unsupported install method: {raw}")),
    }
}

fn parse_step(raw: &str) -> Result<InstallStep, String> {
    match raw {
        "precheck" => Ok(InstallStep::Precheck),
        "install" => Ok(InstallStep::Install),
        "init" => Ok(InstallStep::Init),
        "verify" => Ok(InstallStep::Verify),
        _ => Err(format!("unsupported install step: {raw}")),
    }
}

fn create_session(
    store: &InstallSessionStore,
    method_raw: &str,
    options: Option<HashMap<String, Value>>,
) -> Result<InstallSession, String> {
    let method = parse_method(method_raw)?;
    if !is_method_available(&method) {
        return Err(format!(
            "install method '{}' is unavailable on this platform",
            method.as_str()
        ));
    }
    let now = Utc::now().to_rfc3339();
    let session = InstallSession {
        id: format!("install-{}", Uuid::new_v4()),
        method,
        state: InstallState::SelectedMethod,
        current_step: None,
        logs: vec![],
        artifacts: options.unwrap_or_default(),
        created_at: now.clone(),
        updated_at: now,
    };
    store.insert(session.clone())?;
    Ok(session)
}

fn is_step_allowed(state: &InstallState, step: &InstallStep) -> bool {
    match step {
        InstallStep::Precheck => matches!(
            state,
            InstallState::SelectedMethod | InstallState::PrecheckFailed
        ),
        InstallStep::Install => matches!(
            state,
            InstallState::PrecheckPassed | InstallState::InstallFailed
        ),
        InstallStep::Init => matches!(
            state,
            InstallState::InstallPassed | InstallState::InitFailed
        ),
        InstallStep::Verify => {
            matches!(state, InstallState::InitPassed | InstallState::VerifyFailed)
        }
    }
}

fn running_state(step: &InstallStep) -> InstallState {
    match step {
        InstallStep::Precheck => InstallState::PrecheckRunning,
        InstallStep::Install => InstallState::InstallRunning,
        InstallStep::Init => InstallState::InitRunning,
        InstallStep::Verify => InstallState::VerifyRunning,
    }
}

fn success_state(step: &InstallStep) -> InstallState {
    match step {
        InstallStep::Precheck => InstallState::PrecheckPassed,
        InstallStep::Install => InstallState::InstallPassed,
        InstallStep::Init => InstallState::InitPassed,
        InstallStep::Verify => InstallState::Ready,
    }
}

fn failed_state(step: &InstallStep) -> InstallState {
    match step {
        InstallStep::Precheck => InstallState::PrecheckFailed,
        InstallStep::Install => InstallState::InstallFailed,
        InstallStep::Init => InstallState::InitFailed,
        InstallStep::Verify => InstallState::VerifyFailed,
    }
}

fn next_step(step: &InstallStep) -> Option<String> {
    match step {
        InstallStep::Precheck => Some("install".to_string()),
        InstallStep::Install => Some("init".to_string()),
        InstallStep::Init => Some("verify".to_string()),
        InstallStep::Verify => None,
    }
}

fn is_method_available(method: &InstallMethod) -> bool {
    match method {
        InstallMethod::Local => true,
        InstallMethod::Wsl2 => cfg!(target_os = "windows"),
        InstallMethod::Docker => true,
        InstallMethod::RemoteSsh => true,
    }
}

fn make_result(
    ok: bool,
    summary: String,
    details: String,
    next: Option<String>,
    error_code: Option<String>,
    ssh_diagnostic: Option<SshDiagnosticReport>,
) -> InstallStepResult {
    InstallStepResult {
        ok,
        summary,
        details,
        commands: vec![],
        artifacts: HashMap::<String, Value>::new(),
        next_step: next,
        error_code,
        ssh_diagnostic,
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstallOrchestratorDecision {
    pub step: Option<String>,
    pub reason: String,
    pub source: String,
    pub error_code: Option<String>,
    pub action_hint: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstallUiAction {
    pub id: String,
    pub kind: String,
    pub label: String,
    #[serde(default)]
    pub payload: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstallTargetDecision {
    pub method: Option<String>,
    pub reason: String,
    pub source: String,
    pub requires_ssh_host: bool,
    #[serde(default)]
    pub required_fields: Vec<String>,
    #[serde(default)]
    pub ui_actions: Vec<InstallUiAction>,
    pub error_code: Option<String>,
    pub action_hint: Option<String>,
}

fn context_has_non_empty_string(context: &HashMap<String, Value>, key: &str) -> bool {
    context
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

fn build_goal_context_text(goal: &str, context: &HashMap<String, Value>) -> String {
    let goal_text = goal.trim().to_ascii_lowercase();
    let context_text = serde_json::to_string(context)
        .unwrap_or_else(|_| "{}".to_string())
        .to_ascii_lowercase();
    format!("{goal_text}\n{context_text}")
}

fn text_mentions_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn builtin_target_decision(
    goal: &str,
    context: &HashMap<String, Value>,
) -> Result<InstallTargetDecision, String> {
    let text = build_goal_context_text(goal, context);
    let has_ssh_host = context_has_non_empty_string(context, "ssh_host_id");
    let wants_remote = has_ssh_host
        || text_mentions_any(
            &text,
            &[
                "remote_ssh",
                "ssh",
                "remote",
                "vps",
                "server",
                "hetzner",
                "digitalocean",
                "linode",
            ],
        );
    let wants_docker = text_mentions_any(&text, &["docker", "compose", "container"]);
    let wants_wsl = text_mentions_any(&text, &["wsl2", "\"wsl\"", " wsl ", "windows subsystem"]);

    let (method, reason, requires_ssh_host, required_fields, ui_actions) = if wants_remote {
        if has_ssh_host {
            (
                Some(InstallMethod::RemoteSsh),
                "builtin rules selected remote SSH because an SSH host is already available."
                    .to_string(),
                false,
                Vec::new(),
                Vec::new(),
            )
        } else {
            (
                None,
                "builtin rules detected a remote install target, but no SSH host is selected yet."
                    .to_string(),
                true,
                vec!["ssh_host_id".to_string()],
                vec![InstallUiAction {
                    id: "open-instance-manager".to_string(),
                    kind: "open_instances".to_string(),
                    label: "添加/选择实例".to_string(),
                    payload: HashMap::new(),
                }],
            )
        }
    } else if wants_docker {
        (
            Some(InstallMethod::Docker),
            "builtin rules selected Docker because the goal mentions containers or compose."
                .to_string(),
            false,
            Vec::new(),
            Vec::new(),
        )
    } else if wants_wsl {
        (
            Some(InstallMethod::Wsl2),
            "builtin rules selected WSL2 because the goal mentions WSL.".to_string(),
            false,
            Vec::new(),
            Vec::new(),
        )
    } else {
        (
            Some(InstallMethod::Local),
            "builtin rules selected local because no remote or container target was requested."
                .to_string(),
            false,
            Vec::new(),
            Vec::new(),
        )
    };

    if let Some(parsed_method) = method {
        if !is_method_available(&parsed_method) {
            return Ok(make_target_error_decision(
                format!(
                    "builtin rules proposed unavailable method '{}'",
                    parsed_method.as_str()
                ),
                "builtin-rules",
            ));
        }
        return Ok(InstallTargetDecision {
            method: Some(parsed_method.as_str().to_string()),
            reason,
            source: "builtin-rules".to_string(),
            requires_ssh_host,
            required_fields,
            ui_actions,
            error_code: None,
            action_hint: None,
        });
    }

    Ok(InstallTargetDecision {
        method: None,
        reason,
        source: "builtin-rules".to_string(),
        requires_ssh_host,
        required_fields,
        ui_actions,
        error_code: Some("remote_target_missing".to_string()),
        action_hint: Some("open_instances".to_string()),
    })
}

fn classify_orchestrator_error(raw: &str) -> (String, String) {
    let lower = raw.to_lowercase();
    if lower.contains("no compatible api key found")
        || lower.contains("no auth profile")
        || lower.contains("openrouter_api_key")
        || lower.contains("anthropic_api_key")
        || lower.contains("openai_api_key")
    {
        return ("auth_missing".to_string(), "open_settings_auth".to_string());
    }
    if lower.contains("no ssh host config with id")
        || lower.contains("remote ssh host not found")
        || lower.contains("remote ssh target missing")
    {
        return (
            "remote_target_missing".to_string(),
            "open_instances".to_string(),
        );
    }
    if lower.contains("cannot connect to the docker daemon")
        || lower.contains("docker: command not found")
        || lower.contains("command failed: docker")
    {
        return ("docker_unavailable".to_string(), "open_help".to_string());
    }
    if lower.contains("permission denied") || lower.contains("operation not permitted") {
        return ("permission_denied".to_string(), "open_help".to_string());
    }
    if lower.contains("timed out")
        || lower.contains("network")
        || lower.contains("failed to connect")
        || lower.contains("temporary failure")
    {
        return ("network_error".to_string(), "open_help".to_string());
    }
    ("orchestrator_error".to_string(), "resume".to_string())
}

fn make_target_error_decision(reason: String, source: &str) -> InstallTargetDecision {
    let (error_code, action_hint_raw) = classify_orchestrator_error(&reason);
    let mut ui_actions = Vec::<InstallUiAction>::new();
    let mut required_fields = Vec::<String>::new();
    match action_hint_raw.as_str() {
        "open_settings_auth" => {
            ui_actions.push(InstallUiAction {
                id: "open-settings-auth".to_string(),
                kind: "open_settings".to_string(),
                label: "配置 Auth".to_string(),
                payload: HashMap::new(),
            });
            required_fields.push("auth_profile".to_string());
        }
        "open_instances" => {
            ui_actions.push(InstallUiAction {
                id: "open-instance-manager".to_string(),
                kind: "open_instances".to_string(),
                label: "添加/选择实例".to_string(),
                payload: HashMap::new(),
            });
            required_fields.push("ssh_host_id".to_string());
        }
        "open_help" => {
            ui_actions.push(InstallUiAction {
                id: "open-help".to_string(),
                kind: "open_help".to_string(),
                label: "打开 Help".to_string(),
                payload: HashMap::new(),
            });
        }
        _ => {}
    }
    InstallTargetDecision {
        method: None,
        reason,
        source: source.to_string(),
        requires_ssh_host: false,
        required_fields,
        ui_actions,
        error_code: Some(error_code),
        action_hint: Some(action_hint_raw),
    }
}

fn decide_target_internal(
    goal: &str,
    context: HashMap<String, Value>,
) -> Result<InstallTargetDecision, String> {
    let trimmed_goal = goal.trim();
    if trimmed_goal.is_empty() {
        return Err("goal is required".to_string());
    }
    builtin_target_decision(trimmed_goal, &context)
}

fn orchestrator_next_for_session(
    session: InstallSession,
    goal: &str,
) -> Result<InstallOrchestratorDecision, String> {
    let trimmed_goal = goal.trim();
    if trimmed_goal.is_empty() {
        return Err("goal is required".to_string());
    }
    let (step, reason) = match session.state {
        InstallState::Idle => (
            None,
            "builtin rules are waiting for an installation method to be selected.".to_string(),
        ),
        InstallState::SelectedMethod | InstallState::PrecheckFailed => (
            Some("precheck".to_string()),
            "builtin rules selected the precheck step for the current install state.".to_string(),
        ),
        InstallState::PrecheckRunning => (
            Some("precheck".to_string()),
            "builtin rules kept the precheck step because it is already running.".to_string(),
        ),
        InstallState::PrecheckPassed | InstallState::InstallFailed => (
            Some("install".to_string()),
            "builtin rules selected the install step after precheck.".to_string(),
        ),
        InstallState::InstallRunning => (
            Some("install".to_string()),
            "builtin rules kept the install step because it is already running.".to_string(),
        ),
        InstallState::InstallPassed | InstallState::InitFailed => (
            Some("init".to_string()),
            "builtin rules selected the init step after install.".to_string(),
        ),
        InstallState::InitRunning => (
            Some("init".to_string()),
            "builtin rules kept the init step because it is already running.".to_string(),
        ),
        InstallState::InitPassed | InstallState::VerifyFailed => (
            Some("verify".to_string()),
            "builtin rules selected the verify step after init.".to_string(),
        ),
        InstallState::VerifyRunning => (
            Some("verify".to_string()),
            "builtin rules kept the verify step because it is already running.".to_string(),
        ),
        InstallState::Ready => (
            None,
            "builtin rules determined that the install session is already ready.".to_string(),
        ),
    };

    Ok(InstallOrchestratorDecision {
        step,
        reason,
        source: "builtin-rules".to_string(),
        error_code: None,
        action_hint: None,
    })
}

fn orchestrator_next_internal(
    store: &InstallSessionStore,
    session_id: &str,
    goal: &str,
) -> Result<InstallOrchestratorDecision, String> {
    let id = session_id.trim();
    if id.is_empty() {
        return Err("session_id is required".to_string());
    }
    let session = store
        .get(id)?
        .ok_or_else(|| format!("install session not found: {id}"))?;
    orchestrator_next_for_session(session, goal)
}

fn append_executed_commands(session: &mut InstallSession, commands: &[String]) {
    if commands.is_empty() {
        return;
    }
    let key = "executed_commands".to_string();
    let next_values: Vec<Value> = commands
        .iter()
        .map(|cmd| Value::String(cmd.clone()))
        .collect();
    match session.artifacts.get_mut(&key) {
        Some(Value::Array(existing)) => {
            existing.extend(next_values);
        }
        _ => {
            session.artifacts.insert(key, Value::Array(next_values));
        }
    }
}

fn list_method_capabilities() -> Vec<InstallMethodCapability> {
    vec![
        InstallMethodCapability {
            method: "local".to_string(),
            available: is_method_available(&InstallMethod::Local),
            hint: None,
        },
        InstallMethodCapability {
            method: "wsl2".to_string(),
            available: is_method_available(&InstallMethod::Wsl2),
            hint: Some("Requires WSL2 environment".to_string()),
        },
        InstallMethodCapability {
            method: "docker".to_string(),
            available: is_method_available(&InstallMethod::Docker),
            hint: Some("Requires Docker daemon to be running".to_string()),
        },
        InstallMethodCapability {
            method: "remote_ssh".to_string(),
            available: is_method_available(&InstallMethod::RemoteSsh),
            hint: Some("Requires reachable SSH host".to_string()),
        },
    ]
}

fn make_remote_ssh_runner_failure(
    stage: SshStage,
    summary: &str,
    details: String,
    commands: Vec<String>,
) -> runners::RunnerFailure {
    let ssh_diagnostic = from_any_error(stage, SshIntent::InstallStep, details.clone());
    let error_code = ssh_diagnostic
        .error_code
        .unwrap_or(SshErrorCode::Unknown)
        .as_str()
        .to_string();
    runners::RunnerFailure {
        error_code,
        summary: summary.to_string(),
        details,
        commands,
        ssh_diagnostic: Some(ssh_diagnostic),
    }
}

async fn run_remote_ssh_step(
    pool: &SshConnectionPool,
    host_id: &str,
    step: &InstallStep,
    artifacts: &HashMap<String, Value>,
) -> Result<runners::RunnerOutput, runners::RunnerFailure> {
    let status = if pool.is_connected(host_id).await {
        "connected".to_string()
    } else {
        "disconnected".to_string()
    };
    if status != "connected" {
        let hosts = crate::commands::list_ssh_hosts().map_err(|e| {
            make_remote_ssh_runner_failure(
                SshStage::ResolveHostConfig,
                "remote ssh host lookup failed",
                e,
                vec![],
            )
        })?;
        let host = hosts.into_iter().find(|h| h.id == host_id).ok_or_else(|| {
            make_remote_ssh_runner_failure(
                SshStage::ResolveHostConfig,
                "remote ssh host not found",
                format!("No SSH host config with id: {host_id}"),
                vec![],
            )
        })?;
        pool.connect(&host).await.map_err(|e| {
            make_remote_ssh_runner_failure(
                SshStage::TcpReachability,
                "remote ssh connect failed",
                e,
                vec![format!("connect host {host_id}")],
            )
        })?;
    }
    runners::remote_ssh::run_step(pool, host_id, step, artifacts).await
}

async fn run_step(
    store: &InstallSessionStore,
    pool: Option<&SshConnectionPool>,
    session_id_raw: &str,
    step_raw: &str,
) -> Result<InstallStepResult, String> {
    let session_id = session_id_raw.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }

    let step = match parse_step(step_raw.trim()) {
        Ok(value) => value,
        Err(e) => {
            return Ok(make_result(
                false,
                "Install step rejected".to_string(),
                e,
                None,
                Some("validation_failed".to_string()),
                None,
            ))
        }
    };

    let mut session = match store.get(session_id)? {
        Some(value) => value,
        None => return Err(format!("install session not found: {session_id}")),
    };
    let method = session.method.clone();

    if !is_step_allowed(&session.state, &step) {
        let blocked_state = session.state.as_str().to_string();
        return Ok(make_result(
            false,
            format!("{} blocked", step.as_str()),
            format!("Current state '{blocked_state}' does not allow this step"),
            None,
            Some("validation_failed".to_string()),
            None,
        ));
    }

    session.current_step = Some(step.clone());
    session.state = running_state(&step);
    session.updated_at = Utc::now().to_rfc3339();
    store.upsert(session.clone())?;

    let run_outcome = match method {
        InstallMethod::RemoteSsh => {
            let Some(host_id) = session
                .artifacts
                .get("ssh_host_id")
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            else {
                session.state = failed_state(&step);
                session.updated_at = Utc::now().to_rfc3339();
                store.upsert(session)?;
                return Ok(make_result(
                    false,
                    "remote ssh target missing".to_string(),
                    "Please select an existing remote instance before starting".to_string(),
                    None,
                    Some("validation_failed".to_string()),
                    None,
                ));
            };
            let Some(pool) = pool else {
                session.state = failed_state(&step);
                session.updated_at = Utc::now().to_rfc3339();
                store.upsert(session)?;
                return Ok(make_result(
                    false,
                    "remote ssh unavailable".to_string(),
                    "SSH connection pool is unavailable".to_string(),
                    None,
                    Some("validation_failed".to_string()),
                    None,
                ));
            };
            run_remote_ssh_step(pool, &host_id, &step, &session.artifacts).await
        }
        _ => runners::run_step(&method, &step, &session.artifacts),
    };
    match run_outcome {
        Ok(output) => {
            for (key, value) in &output.artifacts {
                session.artifacts.insert(key.clone(), value.clone());
            }
            append_executed_commands(&mut session, &output.commands);
            session.state = success_state(&step);
            session.updated_at = Utc::now().to_rfc3339();
            store.upsert(session)?;

            let mut result = make_result(
                true,
                output.summary,
                output.details,
                next_step(&step),
                None,
                None,
            );
            result.commands = output.commands;
            result.artifacts = output.artifacts;
            Ok(result)
        }
        Err(err) => {
            session.state = failed_state(&step);
            session.updated_at = Utc::now().to_rfc3339();
            store.upsert(session)?;

            let mut result = make_result(
                false,
                err.summary,
                err.details,
                None,
                Some(err.error_code),
                err.ssh_diagnostic,
            );
            result.commands = err.commands;
            Ok(result)
        }
    }
}

#[tauri::command]
pub async fn install_create_session(
    method: String,
    options: Option<HashMap<String, Value>>,
    store: State<'_, InstallSessionStore>,
) -> Result<InstallSession, String> {
    create_session(&store, method.trim(), options)
}

#[tauri::command]
pub async fn install_get_session(
    session_id: String,
    store: State<'_, InstallSessionStore>,
) -> Result<InstallSession, String> {
    let id = session_id.trim();
    if id.is_empty() {
        return Err("session_id is required".to_string());
    }
    match store.get(id)? {
        Some(session) => Ok(session),
        None => Err(format!("install session not found: {id}")),
    }
}

#[tauri::command]
pub async fn install_run_step(
    session_id: String,
    step: String,
    pool: State<'_, SshConnectionPool>,
    store: State<'_, InstallSessionStore>,
) -> Result<InstallStepResult, String> {
    run_step(&store, Some(&pool), &session_id, &step).await
}

#[tauri::command]
pub async fn install_list_methods() -> Result<Vec<InstallMethodCapability>, String> {
    Ok(list_method_capabilities())
}

#[tauri::command]
pub async fn install_decide_target(
    goal: String,
    context: Option<HashMap<String, Value>>,
) -> Result<InstallTargetDecision, String> {
    let context = context.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || decide_target_internal(&goal, context))
        .await
        .map_err(|e| format!("failed to run install target decider task: {e}"))?
}

#[tauri::command]
pub async fn install_orchestrator_next(
    session_id: String,
    goal: String,
    store: State<'_, InstallSessionStore>,
) -> Result<InstallOrchestratorDecision, String> {
    let id = session_id.trim();
    if id.is_empty() {
        return Err("session_id is required".to_string());
    }
    let session = store
        .get(id)?
        .ok_or_else(|| format!("install session not found: {id}"))?;
    tauri::async_runtime::spawn_blocking(move || orchestrator_next_for_session(session, &goal))
        .await
        .map_err(|e| format!("failed to run install orchestrator task: {e}"))?
}

pub async fn create_session_for_test(method: &str) -> Result<InstallSession, String> {
    create_session(&TEST_SESSION_STORE, method, None)
}

pub async fn get_session_for_test(session_id: &str) -> Result<InstallSession, String> {
    let id = session_id.trim();
    if id.is_empty() {
        return Err("session_id is required".to_string());
    }
    TEST_SESSION_STORE
        .get(id)?
        .ok_or_else(|| format!("install session not found: {id}"))
}

pub async fn run_step_for_test(session_id: &str, step: &str) -> Result<InstallStepResult, String> {
    run_step(&TEST_SESSION_STORE, None, session_id, step).await
}

pub async fn list_methods_for_test() -> Result<Vec<InstallMethodCapability>, String> {
    Ok(list_method_capabilities())
}

pub async fn orchestrator_next_for_test(
    session_id: &str,
    goal: &str,
) -> Result<InstallOrchestratorDecision, String> {
    orchestrator_next_internal(&TEST_SESSION_STORE, session_id, goal)
}

pub async fn run_local_precheck_for_test() -> Result<InstallStepResult, String> {
    let output = runners::run_step(
        &InstallMethod::Local,
        &InstallStep::Precheck,
        &HashMap::new(),
    )
    .map_err(|e| format!("{}: {}", e.summary, e.details))?;
    let mut result = make_result(
        true,
        output.summary,
        output.details,
        next_step(&InstallStep::Precheck),
        None,
        None,
    );
    result.commands = output.commands;
    result.artifacts = output.artifacts;
    Ok(result)
}

pub fn failed_state_for_test(step: &str) -> Result<String, String> {
    let parsed = parse_step(step)?;
    Ok(failed_state(&parsed).as_str().to_string())
}
