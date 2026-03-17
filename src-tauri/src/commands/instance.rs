use super::*;

#[tauri::command]
pub fn set_active_openclaw_home(path: Option<String>) -> Result<bool, String> {
    timed_sync!("set_active_openclaw_home", {
        crate::cli_runner::set_active_openclaw_home_override(path)?;
        Ok(true)
    })
}

#[tauri::command]
pub fn set_active_clawpal_data_dir(path: Option<String>) -> Result<bool, String> {
    timed_sync!("set_active_clawpal_data_dir", {
        crate::cli_runner::set_active_clawpal_data_override(path)?;
        Ok(true)
    })
}

#[tauri::command]
pub fn local_openclaw_config_exists(openclaw_home: String) -> Result<bool, String> {
    timed_sync!("local_openclaw_config_exists", {
        let home = openclaw_home.trim();
        if home.is_empty() {
            return Ok(false);
        }
        let expanded = shellexpand::tilde(home).to_string();
        let config_path = PathBuf::from(expanded)
            .join(".openclaw")
            .join("openclaw.json");
        Ok(config_path.exists())
    })
}

#[tauri::command]
pub fn local_openclaw_cli_available() -> Result<bool, String> {
    timed_sync!("local_openclaw_cli_available", {
        Ok(run_openclaw_raw(&["--version"]).is_ok())
    })
}

#[tauri::command]
pub fn delete_local_instance_home(openclaw_home: String) -> Result<bool, String> {
    timed_sync!("delete_local_instance_home", {
        let home = openclaw_home.trim();
        if home.is_empty() {
            return Err("openclaw_home is required".to_string());
        }
        let expanded = shellexpand::tilde(home).to_string();
        let target = PathBuf::from(expanded);
        if !target.exists() {
            return Ok(true);
        }

        let canonical_target = target
            .canonicalize()
            .map_err(|e| format!("failed to resolve target path: {e}"))?;
        let user_home =
            dirs::home_dir().ok_or_else(|| "failed to resolve HOME directory".to_string())?;
        let allowed_root = user_home.join(".clawpal");
        let canonical_allowed_root = allowed_root
            .canonicalize()
            .map_err(|e| format!("failed to resolve ~/.clawpal path: {e}"))?;

        if !canonical_target.starts_with(&canonical_allowed_root) {
            return Err("refuse to delete path outside ~/.clawpal".to_string());
        }
        if canonical_target == canonical_allowed_root {
            return Err("refuse to delete ~/.clawpal root".to_string());
        }

        fs::remove_dir_all(&canonical_target).map_err(|e| {
            format!(
                "failed to delete '{}': {e}",
                canonical_target.to_string_lossy()
            )
        })?;
        Ok(true)
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsureAccessResult {
    pub instance_id: String,
    pub transport: String,
    pub working_chain: Vec<String>,
    pub used_legacy_fallback: bool,
    pub profile_reused: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordInstallExperienceResult {
    pub saved: bool,
    pub total_count: usize,
}

pub async fn ensure_access_profile_impl(
    instance_id: String,
    transport: String,
) -> Result<EnsureAccessResult, String> {
    let paths = resolve_paths();
    let store = AccessDiscoveryStore::new(paths.clawpal_dir.join("access-discovery"));
    if let Some(existing) = store.load_profile(&instance_id)? {
        if !existing.working_chain.is_empty() {
            return Ok(EnsureAccessResult {
                instance_id,
                transport,
                working_chain: existing.working_chain,
                used_legacy_fallback: false,
                profile_reused: true,
            });
        }
    }

    let probe_plan = build_probe_plan_for_local();
    let probes = probe_plan
        .iter()
        .enumerate()
        .map(|(idx, cmd)| {
            run_probe_with_redaction(&format!("probe-{idx}"), cmd, "planned", true, 0)
        })
        .collect::<Vec<_>>();

    let mut profile = CapabilityProfile::example_local(&instance_id);
    profile.transport = transport.clone();
    profile.probes = probes;
    profile.verified_at = unix_timestamp_secs();

    let used_legacy_fallback = if store.save_profile(&profile).is_err() {
        true
    } else {
        false
    };

    Ok(EnsureAccessResult {
        instance_id,
        transport,
        working_chain: profile.working_chain,
        used_legacy_fallback,
        profile_reused: false,
    })
}

#[tauri::command]
pub async fn ensure_access_profile(
    instance_id: String,
    transport: String,
) -> Result<EnsureAccessResult, String> {
    timed_async!("ensure_access_profile", {
        ensure_access_profile_impl(instance_id, transport).await
    })
}

pub async fn ensure_access_profile_for_test(
    instance_id: &str,
) -> Result<EnsureAccessResult, String> {
    ensure_access_profile_impl(instance_id.to_string(), "local".to_string()).await
}

fn value_array_as_strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[tauri::command]
pub async fn record_install_experience(
    session_id: String,
    instance_id: String,
    goal: String,
    store: State<'_, InstallSessionStore>,
) -> Result<RecordInstallExperienceResult, String> {
    timed_async!("record_install_experience", {
        let id = session_id.trim();
        if id.is_empty() {
            return Err("session_id is required".to_string());
        }
        let session = store
            .get(id)?
            .ok_or_else(|| format!("install session not found: {id}"))?;
        if !matches!(session.state, InstallState::Ready) {
            return Err(format!(
                "install session is not ready: {}",
                session.state.as_str()
            ));
        }

        let transport = session.method.as_str().to_string();
        let paths = resolve_paths();
        let discovery_store = AccessDiscoveryStore::new(paths.clawpal_dir.join("access-discovery"));
        let profile = discovery_store.load_profile(&instance_id)?;
        let successful_chain = profile.map(|p| p.working_chain).unwrap_or_default();
        let commands = value_array_as_strings(session.artifacts.get("executed_commands"));

        let experience = ExecutionExperience {
            instance_id: instance_id.clone(),
            goal,
            transport,
            method: session.method.as_str().to_string(),
            commands,
            successful_chain,
            recorded_at: unix_timestamp_secs(),
        };
        let total_count = discovery_store.save_experience(experience)?;
        Ok(RecordInstallExperienceResult {
            saved: true,
            total_count,
    })
    })
}

#[tauri::command]
pub fn list_registered_instances() -> Result<Vec<clawpal_core::instance::Instance>, String> {
    timed_sync!("list_registered_instances", {
        let registry = clawpal_core::instance::InstanceRegistry::load().map_err(|e| e.to_string())?;
        // Best-effort self-heal: persist normalized instance ids (e.g., legacy empty SSH ids).
        let _ = registry.save();
        Ok(registry.list())
    })
}

#[tauri::command]
pub fn delete_registered_instance(instance_id: String) -> Result<bool, String> {
    timed_sync!("delete_registered_instance", {
        let id = instance_id.trim();
        if id.is_empty() || id == "local" {
            return Ok(false);
        }
        let mut registry =
            clawpal_core::instance::InstanceRegistry::load().map_err(|e| e.to_string())?;
        let removed = registry.remove(id).is_some();
        if removed {
            registry.save().map_err(|e| e.to_string())?;
        }
        Ok(removed)
    })
}

#[tauri::command]
pub async fn connect_docker_instance(
    home: String,
    label: Option<String>,
    instance_id: Option<String>,
) -> Result<clawpal_core::instance::Instance, String> {
    timed_async!("connect_docker_instance", {
        clawpal_core::connect::connect_docker(&home, label.as_deref(), instance_id.as_deref())
            .await
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub async fn connect_local_instance(
    home: String,
    label: Option<String>,
    instance_id: Option<String>,
) -> Result<clawpal_core::instance::Instance, String> {
    timed_async!("connect_local_instance", {
        clawpal_core::connect::connect_local(&home, label.as_deref(), instance_id.as_deref())
            .await
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub async fn connect_ssh_instance(
    host_id: String,
) -> Result<clawpal_core::instance::Instance, String> {
    timed_async!("connect_ssh_instance", {
        let hosts = read_hosts_from_registry()?;
        let host = hosts
            .into_iter()
            .find(|h| h.id == host_id)
            .ok_or_else(|| format!("No SSH host config with id: {host_id}"))?;
        // Register the SSH host as an instance in the instance registry
        // (skip the actual SSH connectivity probe — the caller already connected)
        let instance = clawpal_core::instance::Instance {
            id: host.id.clone(),
            instance_type: clawpal_core::instance::InstanceType::RemoteSsh,
            label: host.label.clone(),
            openclaw_home: None,
            clawpal_data_dir: None,
            ssh_host_config: Some(host),
        };
        let mut registry =
            clawpal_core::instance::InstanceRegistry::load().map_err(|e| e.to_string())?;
        let _ = registry.remove(&instance.id);
        registry.add(instance.clone()).map_err(|e| e.to_string())?;
        registry.save().map_err(|e| e.to_string())?;
        Ok(instance)
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyDockerInstance {
    pub id: String,
    pub label: String,
    pub openclaw_home: Option<String>,
    pub clawpal_data_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyMigrationResult {
    pub imported_ssh_hosts: usize,
    pub imported_docker_instances: usize,
    pub imported_open_tab_instances: usize,
    pub total_instances: usize,
}

fn fallback_label_from_instance_id(instance_id: &str) -> String {
    if instance_id == "local" {
        return "Local".to_string();
    }
    if let Some(suffix) = instance_id.strip_prefix("docker:") {
        if suffix.is_empty() {
            return "docker-local".to_string();
        }
        if suffix.starts_with("docker-") {
            return suffix.to_string();
        }
        return format!("docker-{suffix}");
    }
    if let Some(suffix) = instance_id.strip_prefix("ssh:") {
        return if suffix.is_empty() {
            "SSH".to_string()
        } else {
            suffix.to_string()
        };
    }
    instance_id.to_string()
}

fn upsert_registry_instance(
    registry: &mut clawpal_core::instance::InstanceRegistry,
    instance: clawpal_core::instance::Instance,
) -> Result<(), String> {
    let _ = registry.remove(&instance.id);
    registry.add(instance).map_err(|e| e.to_string())
}

fn migrate_legacy_ssh_file(
    paths: &crate::models::OpenClawPaths,
    registry: &mut clawpal_core::instance::InstanceRegistry,
) -> Result<usize, String> {
    let legacy_path = paths.clawpal_dir.join("remote-instances.json");
    if !legacy_path.exists() {
        return Ok(0);
    }
    let text = fs::read_to_string(&legacy_path).map_err(|e| e.to_string())?;
    let hosts: Vec<SshHostConfig> = serde_json::from_str(&text).unwrap_or_default();
    let mut count = 0usize;
    for host in hosts {
        let instance = clawpal_core::instance::Instance {
            id: host.id.clone(),
            instance_type: clawpal_core::instance::InstanceType::RemoteSsh,
            label: if host.label.trim().is_empty() {
                host.host.clone()
            } else {
                host.label.clone()
            },
            openclaw_home: None,
            clawpal_data_dir: None,
            ssh_host_config: Some(host),
        };
        upsert_registry_instance(registry, instance)?;
        count += 1;
    }
    // Remove legacy file after successful migration so it doesn't
    // re-add deleted hosts on subsequent page loads.
    if count > 0 {
        let _ = fs::remove_file(&legacy_path);
    }
    Ok(count)
}

#[tauri::command]
pub fn migrate_legacy_instances(
    legacy_docker_instances: Vec<LegacyDockerInstance>,
    legacy_open_tab_ids: Vec<String>,
) -> Result<LegacyMigrationResult, String> {
    timed_sync!("migrate_legacy_instances", {
        let paths = resolve_paths();
        let mut registry =
            clawpal_core::instance::InstanceRegistry::load().map_err(|e| e.to_string())?;

        // Ensure local instance exists for old users.
        if registry.get("local").is_none() {
            upsert_registry_instance(
                &mut registry,
                clawpal_core::instance::Instance {
                    id: "local".to_string(),
                    instance_type: clawpal_core::instance::InstanceType::Local,
                    label: "Local".to_string(),
                    openclaw_home: None,
                    clawpal_data_dir: None,
                    ssh_host_config: None,
                },
            )?;
        }

        let imported_ssh_hosts = migrate_legacy_ssh_file(&paths, &mut registry)?;

        let mut imported_docker_instances = 0usize;
        for docker in legacy_docker_instances {
            let id = docker.id.trim();
            if id.is_empty() {
                continue;
            }
            let label = if docker.label.trim().is_empty() {
                fallback_label_from_instance_id(id)
            } else {
                docker.label.clone()
            };
            upsert_registry_instance(
                &mut registry,
                clawpal_core::instance::Instance {
                    id: id.to_string(),
                    instance_type: clawpal_core::instance::InstanceType::Docker,
                    label,
                    openclaw_home: docker.openclaw_home.clone(),
                    clawpal_data_dir: docker.clawpal_data_dir.clone(),
                    ssh_host_config: None,
                },
            )?;
            imported_docker_instances += 1;
        }

        let mut imported_open_tab_instances = 0usize;
        for tab_id in legacy_open_tab_ids {
            let id = tab_id.trim();
            if id.is_empty() {
                continue;
            }
            if registry.get(id).is_some() {
                continue;
            }
            if id == "local" {
                continue;
            }
            if id.starts_with("docker:") {
                upsert_registry_instance(
                    &mut registry,
                    clawpal_core::instance::Instance {
                        id: id.to_string(),
                        instance_type: clawpal_core::instance::InstanceType::Docker,
                        label: fallback_label_from_instance_id(id),
                        openclaw_home: None,
                        clawpal_data_dir: None,
                        ssh_host_config: None,
                    },
                )?;
                imported_open_tab_instances += 1;
                continue;
            }
            if id.starts_with("ssh:") {
                let host_alias = id.strip_prefix("ssh:").unwrap_or("").to_string();
                upsert_registry_instance(
                    &mut registry,
                    clawpal_core::instance::Instance {
                        id: id.to_string(),
                        instance_type: clawpal_core::instance::InstanceType::RemoteSsh,
                        label: fallback_label_from_instance_id(id),
                        openclaw_home: None,
                        clawpal_data_dir: None,
                        ssh_host_config: Some(clawpal_core::instance::SshHostConfig {
                            id: id.to_string(),
                            label: fallback_label_from_instance_id(id),
                            host: host_alias,
                            port: 22,
                            username: String::new(),
                            auth_method: "ssh_config".to_string(),
                            key_path: None,
                            password: None,
                            passphrase: None,
                        }),
                    },
                )?;
                imported_open_tab_instances += 1;
            }
        }

        registry.save().map_err(|e| e.to_string())?;
        let total_instances = registry.list().len();
        Ok(LegacyMigrationResult {
            imported_ssh_hosts,
            imported_docker_instances,
            imported_open_tab_instances,
            total_instances,
    })
    })
}
