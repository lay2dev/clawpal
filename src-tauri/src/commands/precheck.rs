use clawpal_core::precheck::{self, PrecheckIssue};
use serde_json::json;
use tauri::{AppHandle, Emitter, State};

use crate::ssh::SshConnectionPool;

fn merge_auth_precheck_issues(
    profiles: &[clawpal_core::profile::ModelProfile],
    resolved_keys: &[super::ResolvedApiKey],
) -> Vec<PrecheckIssue> {
    let mut issues = precheck::precheck_auth(profiles);
    for profile in profiles {
        if !profile.enabled {
            continue;
        }
        if profile.provider.trim().is_empty() || profile.model.trim().is_empty() {
            continue;
        }
        if super::provider_supports_optional_api_key(&profile.provider) {
            continue;
        }

        let resolved = resolved_keys
            .iter()
            .find(|item| item.profile_id == profile.id);
        if resolved.is_some_and(|item| item.resolved) {
            continue;
        }

        issues.push(PrecheckIssue {
            code: "AUTH_CREDENTIAL_UNRESOLVED".into(),
            severity: "error".into(),
            message: format!(
                "Profile '{}' has no resolved credential for provider '{}'",
                profile.id, profile.provider
            ),
            auto_fixable: false,
        });
    }
    issues
}

fn emit_cook_activity(
    app: &AppHandle,
    session_id: Option<&str>,
    instance_id: &str,
    id: &str,
    label: &str,
    status: &str,
    side_effect: bool,
    target: Option<&str>,
    display_command: Option<&str>,
    started_at: &str,
    finished_at: Option<&str>,
    details: Option<String>,
) {
    let Some(session_id) = session_id else {
        return;
    };

    let _ = app.emit(
        "cook:activity",
        json!({
            "id": id,
            "sessionId": session_id,
            "instanceId": instance_id,
            "phase": "planning.auth",
            "kind": "auth_check",
            "label": label,
            "status": status,
            "sideEffect": side_effect,
            "target": target,
            "displayCommand": display_command,
            "startedAt": started_at,
            "finishedAt": finished_at,
            "details": details,
        }),
    );
}

#[tauri::command]
pub async fn precheck_registry() -> Result<Vec<PrecheckIssue>, String> {
    let registry_path = clawpal_core::instance::registry_path();
    Ok(precheck::precheck_registry(&registry_path))
}

#[tauri::command]
pub async fn precheck_instance(instance_id: String) -> Result<Vec<PrecheckIssue>, String> {
    let registry = clawpal_core::instance::InstanceRegistry::load().map_err(|e| e.to_string())?;
    let instance = registry
        .get(&instance_id)
        .ok_or_else(|| format!("Instance not found: {instance_id}"))?;
    Ok(precheck::precheck_instance_state(instance))
}

#[tauri::command]
pub async fn precheck_transport(
    pool: State<'_, SshConnectionPool>,
    instance_id: String,
) -> Result<Vec<PrecheckIssue>, String> {
    let registry = clawpal_core::instance::InstanceRegistry::load().map_err(|e| e.to_string())?;
    let instance = registry
        .get(&instance_id)
        .ok_or_else(|| format!("Instance not found: {instance_id}"))?;

    let mut issues = Vec::new();

    match &instance.instance_type {
        clawpal_core::instance::InstanceType::RemoteSsh => {
            if !pool.is_connected(&instance_id).await {
                issues.push(PrecheckIssue {
                    code: "TRANSPORT_STALE".into(),
                    severity: "warn".into(),
                    message: format!(
                        "SSH connection for instance '{}' is not active",
                        instance.label
                    ),
                    auto_fixable: false,
                });
            }
        }
        clawpal_core::instance::InstanceType::Docker => {
            let docker_ok = tokio::process::Command::new("docker")
                .args(["info", "--format", "{{.ServerVersion}}"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await
                .map(|s| s.success())
                .unwrap_or(false);
            if !docker_ok {
                issues.push(PrecheckIssue {
                    code: "TRANSPORT_STALE".into(),
                    severity: "error".into(),
                    message: "Docker daemon is not running or unreachable".into(),
                    auto_fixable: false,
                });
            }
        }
        _ => {}
    }

    Ok(issues)
}

#[tauri::command]
pub async fn precheck_auth(
    app: AppHandle,
    pool: State<'_, SshConnectionPool>,
    instance_id: String,
    activity_session_id: Option<String>,
) -> Result<Vec<PrecheckIssue>, String> {
    let registry = clawpal_core::instance::InstanceRegistry::load().map_err(|e| e.to_string())?;
    let instance = registry
        .get(&instance_id)
        .ok_or_else(|| format!("Instance not found: {instance_id}"))?;

    match &instance.instance_type {
        clawpal_core::instance::InstanceType::RemoteSsh => {
            let session_id = activity_session_id.as_deref();
            let collect_id = format!("{}:planning:auth:profiles", instance_id);
            let collect_started_at = chrono::Utc::now().to_rfc3339();
            emit_cook_activity(
                &app,
                session_id,
                &instance_id,
                &collect_id,
                "Collect remote model profiles",
                "started",
                false,
                Some("remote OpenClaw config"),
                Some("Read remote openclaw.json and ~/.clawpal/model-profiles.json"),
                &collect_started_at,
                None,
                None,
            );
            let (profiles, extract_result) =
                super::profiles::collect_remote_profiles_from_openclaw(&pool, &instance_id, true)
                    .await
                    .map_err(|error| {
                        emit_cook_activity(
                            &app,
                            session_id,
                            &instance_id,
                            &collect_id,
                            "Collect remote model profiles",
                            "failed",
                            false,
                            Some("remote OpenClaw config"),
                            Some("Read remote openclaw.json and ~/.clawpal/model-profiles.json"),
                            &collect_started_at,
                            Some(&chrono::Utc::now().to_rfc3339()),
                            Some(error.clone()),
                        );
                        error
                    })?;
            emit_cook_activity(
                &app,
                session_id,
                &instance_id,
                &collect_id,
                "Collect remote model profiles",
                "succeeded",
                false,
                Some("remote OpenClaw config"),
                Some("Read remote openclaw.json and ~/.clawpal/model-profiles.json"),
                &collect_started_at,
                Some(&chrono::Utc::now().to_rfc3339()),
                Some(format!("Loaded {} profile(s).", profiles.len())),
            );
            if extract_result.created > 0 {
                let sync_id = format!("{}:planning:auth:profile-cache", instance_id);
                let sync_started_at = chrono::Utc::now().to_rfc3339();
                emit_cook_activity(
                    &app,
                    session_id,
                    &instance_id,
                    &sync_id,
                    "Sync derived profile cache",
                    "started",
                    true,
                    Some("~/.clawpal/model-profiles.json"),
                    Some("mkdir -p ~/.clawpal && write ~/.clawpal/model-profiles.json"),
                    &sync_started_at,
                    None,
                    None,
                );
                emit_cook_activity(
                    &app,
                    session_id,
                    &instance_id,
                    &sync_id,
                    "Sync derived profile cache",
                    "succeeded",
                    true,
                    Some("~/.clawpal/model-profiles.json"),
                    Some("mkdir -p ~/.clawpal && write ~/.clawpal/model-profiles.json"),
                    &sync_started_at,
                    Some(&chrono::Utc::now().to_rfc3339()),
                    Some(format!(
                        "Persisted {} newly derived profile(s) for future checks.",
                        extract_result.created
                    )),
                );
            }
            let resolve_id = format!("{}:planning:auth:resolve", instance_id);
            let resolve_started_at = chrono::Utc::now().to_rfc3339();
            emit_cook_activity(
                &app,
                session_id,
                &instance_id,
                &resolve_id,
                "Resolve provider credentials",
                "started",
                false,
                Some(instance.label.as_str()),
                Some("Inspect remote auth store and environment"),
                &resolve_started_at,
                None,
                None,
            );
            let resolved = super::profiles::resolve_remote_api_keys_for_profiles(
                &pool,
                &instance_id,
                &profiles,
            )
            .await;
            emit_cook_activity(
                &app,
                session_id,
                &instance_id,
                &resolve_id,
                "Resolve provider credentials",
                "succeeded",
                false,
                Some(instance.label.as_str()),
                Some("Inspect remote auth store and environment"),
                &resolve_started_at,
                Some(&chrono::Utc::now().to_rfc3339()),
                Some(format!("Checked {} profile(s).", profiles.len())),
            );
            Ok(merge_auth_precheck_issues(&profiles, &resolved))
        }
        _ => {
            let session_id = activity_session_id.as_deref();
            let resolve_id = format!("{}:planning:auth:local", instance_id);
            let resolve_started_at = chrono::Utc::now().to_rfc3339();
            emit_cook_activity(
                &app,
                session_id,
                &instance_id,
                &resolve_id,
                "Resolve provider credentials",
                "started",
                false,
                Some("local shell"),
                Some("Inspect local model profiles and auth environment"),
                &resolve_started_at,
                None,
                None,
            );
            let openclaw = clawpal_core::openclaw::OpenclawCli::new();
            let profiles = clawpal_core::profile::list_profiles(&openclaw).map_err(|e| {
                let message = e.to_string();
                emit_cook_activity(
                    &app,
                    session_id,
                    &instance_id,
                    &resolve_id,
                    "Resolve provider credentials",
                    "failed",
                    false,
                    Some("local shell"),
                    Some("Inspect local model profiles and auth environment"),
                    &resolve_started_at,
                    Some(&chrono::Utc::now().to_rfc3339()),
                    Some(message.clone()),
                );
                message
            })?;
            let resolved = super::resolve_api_keys().map_err(|error| {
                emit_cook_activity(
                    &app,
                    session_id,
                    &instance_id,
                    &resolve_id,
                    "Resolve provider credentials",
                    "failed",
                    false,
                    Some("local shell"),
                    Some("Inspect local model profiles and auth environment"),
                    &resolve_started_at,
                    Some(&chrono::Utc::now().to_rfc3339()),
                    Some(error.clone()),
                );
                error
            })?;
            emit_cook_activity(
                &app,
                session_id,
                &instance_id,
                &resolve_id,
                "Resolve provider credentials",
                "succeeded",
                false,
                Some("local shell"),
                Some("Inspect local model profiles and auth environment"),
                &resolve_started_at,
                Some(&chrono::Utc::now().to_rfc3339()),
                Some(format!("Checked {} profile(s).", profiles.len())),
            );
            Ok(merge_auth_precheck_issues(&profiles, &resolved))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::merge_auth_precheck_issues;
    use crate::commands::{ResolvedApiKey, ResolvedCredentialKind};
    use clawpal_core::profile::ModelProfile;

    fn profile(id: &str, provider: &str, model: &str) -> ModelProfile {
        ModelProfile {
            id: id.into(),
            name: format!("{provider}/{model}"),
            provider: provider.into(),
            model: model.into(),
            auth_ref: "OPENAI_API_KEY".into(),
            api_key: None,
            base_url: None,
            description: None,
            enabled: true,
        }
    }

    #[test]
    fn auth_precheck_detects_unresolved_required_credentials() {
        let issues = merge_auth_precheck_issues(
            &[profile("p1", "openai", "gpt-4o")],
            &[ResolvedApiKey {
                profile_id: "p1".into(),
                masked_key: "not set".into(),
                credential_kind: ResolvedCredentialKind::Unset,
                auth_ref: Some("OPENAI_API_KEY".into()),
                resolved: false,
            }],
        );

        assert!(issues
            .iter()
            .any(|issue| issue.code == "AUTH_CREDENTIAL_UNRESOLVED"));
    }

    #[test]
    fn auth_precheck_skips_optional_api_key_providers() {
        let issues = merge_auth_precheck_issues(
            &[profile("p1", "ollama", "llama3")],
            &[ResolvedApiKey {
                profile_id: "p1".into(),
                masked_key: "not set".into(),
                credential_kind: ResolvedCredentialKind::Unset,
                auth_ref: None,
                resolved: false,
            }],
        );

        assert!(!issues
            .iter()
            .any(|issue| issue.code == "AUTH_CREDENTIAL_UNRESOLVED"));
    }
}
