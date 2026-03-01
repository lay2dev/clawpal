use clawpal_core::precheck::{self, PrecheckIssue};
use tauri::State;

use crate::ssh::SshConnectionPool;

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
pub async fn precheck_auth(instance_id: String) -> Result<Vec<PrecheckIssue>, String> {
    let openclaw = clawpal_core::openclaw::OpenclawCli::new();
    let profiles = clawpal_core::profile::list_profiles(&openclaw).map_err(|e| e.to_string())?;
    let _ = instance_id; // reserved for future per-instance profile filtering
    Ok(precheck::precheck_auth(&profiles))
}
