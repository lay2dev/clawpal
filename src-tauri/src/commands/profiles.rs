use super::*;

fn local_global_openclaw_base_dir() -> std::path::PathBuf {
    resolve_paths().base_dir
}

fn normalize_profile_key(profile: &ModelProfile) -> String {
    normalize_model_ref(&profile_to_model_value(profile))
}

fn is_non_empty(opt: Option<&str>) -> bool {
    opt.map(str::trim).is_some_and(|v| !v.is_empty())
}

fn profile_quality_score(profile: &ModelProfile) -> usize {
    let mut score = 0usize;
    if is_non_empty(profile.api_key.as_deref()) {
        score += 8;
    }
    if !profile.auth_ref.trim().is_empty() {
        score += 4;
    }
    if is_non_empty(profile.base_url.as_deref()) {
        score += 2;
    }
    if profile.enabled {
        score += 1;
    }
    score
}

fn dedupe_profiles_by_model_key(profiles: Vec<ModelProfile>) -> Vec<ModelProfile> {
    let mut deduped: Vec<ModelProfile> = Vec::new();
    let mut key_index: HashMap<String, usize> = HashMap::new();

    for profile in profiles {
        let key = normalize_profile_key(&profile);
        if key.is_empty() {
            deduped.push(profile);
            continue;
        }
        if let Some(existing_idx) = key_index.get(&key).copied() {
            let existing_score = profile_quality_score(&deduped[existing_idx]);
            let incoming_score = profile_quality_score(&profile);
            if incoming_score > existing_score {
                deduped[existing_idx] = profile;
            }
        } else {
            key_index.insert(key, deduped.len());
            deduped.push(profile);
        }
    }

    deduped
}

fn merge_remote_profile_into_local(
    local_profiles: &mut Vec<ModelProfile>,
    remote: &ModelProfile,
    resolved_api_key: Option<String>,
    resolved_base_url: Option<String>,
) -> bool {
    let remote_key = normalize_profile_key(remote);
    let target_idx = local_profiles
        .iter()
        .position(|candidate| candidate.id == remote.id)
        .or_else(|| {
            if remote_key.is_empty() {
                None
            } else {
                local_profiles
                    .iter()
                    .position(|candidate| normalize_profile_key(candidate) == remote_key)
            }
        });

    if let Some(idx) = target_idx {
        let existing = &mut local_profiles[idx];
        if existing.name.trim().is_empty() && !remote.name.trim().is_empty() {
            existing.name = remote.name.clone();
        }
        if existing
            .description
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            existing.description = remote.description.clone();
        }
        if existing.provider.trim().is_empty() && !remote.provider.trim().is_empty() {
            existing.provider = remote.provider.clone();
        }
        if existing.model.trim().is_empty() && !remote.model.trim().is_empty() {
            existing.model = remote.model.clone();
        }
        if existing.auth_ref.trim().is_empty() && !remote.auth_ref.trim().is_empty() {
            existing.auth_ref = remote.auth_ref.clone();
        }
        if !is_non_empty(existing.base_url.as_deref()) && is_non_empty(remote.base_url.as_deref()) {
            existing.base_url = remote.base_url.clone();
        }
        if !is_non_empty(existing.base_url.as_deref()) && is_non_empty(resolved_base_url.as_deref())
        {
            existing.base_url = resolved_base_url;
        }
        if is_non_empty(resolved_api_key.as_deref()) {
            existing.api_key = resolved_api_key;
        } else if !is_non_empty(existing.api_key.as_deref())
            && is_non_empty(remote.api_key.as_deref())
        {
            existing.api_key = remote.api_key.clone();
        }
        if !existing.enabled && remote.enabled {
            existing.enabled = true;
        }
        return false;
    }

    let mut merged = remote.clone();
    if is_non_empty(resolved_api_key.as_deref()) {
        merged.api_key = resolved_api_key;
    }
    if !is_non_empty(merged.base_url.as_deref()) && is_non_empty(resolved_base_url.as_deref()) {
        merged.base_url = resolved_base_url;
    }
    local_profiles.push(merged);
    true
}

#[tauri::command]
pub async fn remote_list_model_profiles(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<ModelProfile>, String> {
    let content = pool
        .sftp_read(&host_id, "~/.clawpal/model-profiles.json")
        .await
        .unwrap_or_else(|_| r#"{"profiles":[]}"#.to_string());
    Ok(clawpal_core::profile::list_profiles_from_storage_json(
        &content,
    ))
}

#[tauri::command]
pub async fn remote_upsert_model_profile(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    profile: ModelProfile,
) -> Result<ModelProfile, String> {
    let content = pool
        .sftp_read(&host_id, "~/.clawpal/model-profiles.json")
        .await
        .unwrap_or_else(|_| r#"{"profiles":[]}"#.to_string());
    let (saved, next_json) =
        clawpal_core::profile::upsert_profile_in_storage_json(&content, profile)
            .map_err(|e| e.to_string())?;

    let _ = pool.exec(&host_id, "mkdir -p ~/.clawpal").await;
    pool.sftp_write(&host_id, "~/.clawpal/model-profiles.json", &next_json)
        .await?;
    Ok(saved)
}

#[tauri::command]
pub async fn remote_delete_model_profile(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    profile_id: String,
) -> Result<bool, String> {
    let content = pool
        .sftp_read(&host_id, "~/.clawpal/model-profiles.json")
        .await
        .unwrap_or_else(|_| r#"{"profiles":[]}"#.to_string());
    let (removed, next_json) =
        clawpal_core::profile::delete_profile_from_storage_json(&content, &profile_id)
            .map_err(|e| e.to_string())?;
    if !removed {
        return Ok(false);
    }
    pool.sftp_write(&host_id, "~/.clawpal/model-profiles.json", &next_json)
        .await?;
    Ok(true)
}

#[tauri::command]
pub async fn remote_resolve_api_keys(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<ResolvedApiKey>, String> {
    let content = match pool
        .sftp_read(&host_id, "~/.clawpal/model-profiles.json")
        .await
    {
        Ok(content) => content,
        Err(e) if is_remote_missing_path_error(&e) => r#"{"profiles":[]}"#.to_string(),
        Err(e) => return Err(format!("Failed to read remote model profiles: {e}")),
    };
    let profiles = clawpal_core::profile::list_profiles_from_storage_json(&content);
    let mut out = Vec::new();
    for profile in &profiles {
        let masked = if let Some(ref key) = profile.api_key {
            if key.len() > 8 {
                format!("{}...{}", &key[..4], &key[key.len() - 4..])
            } else if !key.is_empty() {
                "****".to_string()
            } else if !profile.auth_ref.is_empty() {
                format!("via {}", profile.auth_ref)
            } else {
                "not set".to_string()
            }
        } else if !profile.auth_ref.is_empty() {
            format!("via {}", profile.auth_ref)
        } else {
            "not set".to_string()
        };
        out.push(ResolvedApiKey {
            profile_id: profile.id.clone(),
            masked_key: masked,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn remote_test_model_profile(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    profile_id: String,
) -> Result<bool, String> {
    let content = match pool
        .sftp_read(&host_id, "~/.clawpal/model-profiles.json")
        .await
    {
        Ok(content) => content,
        Err(e) if is_remote_missing_path_error(&e) => r#"{"profiles":[]}"#.to_string(),
        Err(e) => return Err(format!("Failed to read remote model profiles: {e}")),
    };
    let profile = clawpal_core::profile::find_profile_in_storage_json(&content, &profile_id)
        .map_err(|e| format!("Failed to parse remote model profiles: {e}"))?
        .ok_or_else(|| format!("Profile not found: {profile_id}"))?;

    if !profile.enabled {
        return Err("Profile is disabled".into());
    }

    let api_key = resolve_remote_profile_api_key(&pool, &host_id, &profile).await?;
    if api_key.trim().is_empty() && !provider_supports_optional_api_key(&profile.provider) {
        let provider = profile.provider.trim().to_ascii_lowercase();
        let hint = if provider == "anthropic" {
            " For Claude setup-token, also try exporting ANTHROPIC_OAUTH_TOKEN or ANTHROPIC_AUTH_TOKEN on the remote host."
        } else {
            ""
        };
        return Err(
            format!("No API key resolved for this remote profile. Set apiKey directly, configure auth_ref in remote auth store (auth-profiles.json/auth.json), or export auth_ref on remote shell.{hint}"),
        );
    }

    let resolved_base_url = resolve_remote_profile_base_url(&pool, &host_id, &profile).await?;

    tauri::async_runtime::spawn_blocking(move || {
        run_provider_probe(profile.provider, profile.model, resolved_base_url, api_key)
    })
    .await
    .map_err(|e| format!("Task join failed: {e}"))??;

    Ok(true)
}

#[tauri::command]
pub async fn remote_extract_model_profiles_from_config(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<ExtractModelProfilesResult, String> {
    let (_config_path, _raw, cfg) =
        remote_read_openclaw_config_text_and_json(&pool, &host_id).await?;

    let profiles_raw = pool
        .sftp_read(&host_id, "~/.clawpal/model-profiles.json")
        .await
        .unwrap_or_else(|_| r#"{"profiles":[]}"#.to_string());
    let profiles = clawpal_core::profile::list_profiles_from_storage_json(&profiles_raw);

    let bindings = collect_model_bindings(&cfg, &profiles);
    let mut created = 0usize;
    let mut reused = 0usize;
    let mut skipped_invalid = 0usize;
    let mut seen = HashSet::new();

    let mut next_profiles = profiles;
    let mut model_profile_map: HashMap<String, String> = HashMap::new();
    for profile in &next_profiles {
        model_profile_map.insert(
            normalize_model_ref(&profile_to_model_value(profile)),
            profile.id.clone(),
        );
    }

    for binding in bindings {
        let scope_label = match binding.scope.as_str() {
            "global" => "global".to_string(),
            "agent" => format!("agent:{}", binding.scope_id),
            "channel" => format!("channel:{}", binding.scope_id),
            _ => binding.scope_id,
        };
        let Some(model_ref) = binding.model_value else {
            continue;
        };
        let model_ref = normalize_model_ref(&model_ref);
        if model_ref.trim().is_empty() {
            continue;
        }
        if model_profile_map.contains_key(&model_ref) || seen.contains(&model_ref) {
            reused += 1;
            continue;
        }
        let mut parts = model_ref.splitn(2, '/');
        let provider = parts.next().unwrap_or("").trim();
        let model = parts.next().unwrap_or("").trim();
        if provider.is_empty() || model.is_empty() {
            skipped_invalid += 1;
            continue;
        }
        let auth_ref = resolve_auth_ref_for_provider(&cfg, provider)
            .unwrap_or_else(|| format!("{provider}:default"));
        let base_url = resolve_model_provider_base_url(&cfg, provider);
        let new_profile = ModelProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("{scope_label} model profile"),
            provider: provider.to_string(),
            model: model.to_string(),
            auth_ref,
            api_key: None,
            base_url,
            description: Some(format!("Extracted from config ({scope_label})")),
            enabled: true,
        };
        let key = profile_to_model_value(&new_profile);
        model_profile_map.insert(normalize_model_ref(&key), new_profile.id.clone());
        next_profiles.push(new_profile);
        seen.insert(model_ref);
        created += 1;
    }

    if created > 0 {
        let text = clawpal_core::profile::render_profiles_storage_json(&next_profiles)
            .map_err(|e| e.to_string())?;
        let _ = pool.exec(&host_id, "mkdir -p ~/.clawpal").await;
        pool.sftp_write(&host_id, "~/.clawpal/model-profiles.json", &text)
            .await?;
    }

    Ok(ExtractModelProfilesResult {
        created,
        reused,
        skipped_invalid,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAuthSyncResult {
    pub total_remote_profiles: usize,
    pub synced_profiles: usize,
    pub created_profiles: usize,
    pub updated_profiles: usize,
    pub resolved_keys: usize,
    pub unresolved_keys: usize,
    pub failed_key_resolves: usize,
}

#[tauri::command]
pub async fn remote_sync_profiles_to_local_auth(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<RemoteAuthSyncResult, String> {
    let content = match pool
        .sftp_read(&host_id, "~/.clawpal/model-profiles.json")
        .await
    {
        Ok(content) => content,
        Err(e) if is_remote_missing_path_error(&e) => r#"{"profiles":[]}"#.to_string(),
        Err(e) => return Err(format!("Failed to read remote model profiles: {e}")),
    };
    let remote_profiles = clawpal_core::profile::list_profiles_from_storage_json(&content);
    if remote_profiles.is_empty() {
        return Ok(RemoteAuthSyncResult {
            total_remote_profiles: 0,
            synced_profiles: 0,
            created_profiles: 0,
            updated_profiles: 0,
            resolved_keys: 0,
            unresolved_keys: 0,
            failed_key_resolves: 0,
        });
    }

    let paths = resolve_paths();
    let mut local_profiles = dedupe_profiles_by_model_key(load_model_profiles(&paths));

    let mut created_profiles = 0usize;
    let mut updated_profiles = 0usize;
    let mut resolved_keys = 0usize;
    let mut unresolved_keys = 0usize;
    let mut failed_key_resolves = 0usize;

    for remote in &remote_profiles {
        let mut resolved_api_key: Option<String> = None;
        match resolve_remote_profile_api_key(&pool, &host_id, remote).await {
            Ok(api_key) if !api_key.trim().is_empty() => {
                resolved_api_key = Some(api_key);
                resolved_keys += 1;
            }
            Ok(_) => {
                unresolved_keys += 1;
            }
            Err(_) => {
                failed_key_resolves += 1;
            }
        }

        let resolved_base_url = if remote
            .base_url
            .as_deref()
            .map(str::trim)
            .is_some_and(|v| !v.is_empty())
        {
            None
        } else {
            match resolve_remote_profile_base_url(&pool, &host_id, remote).await {
                Ok(Some(remote_base)) if !remote_base.trim().is_empty() => {
                    Some(remote_base.trim().to_string())
                }
                _ => None,
            }
        };

        if merge_remote_profile_into_local(
            &mut local_profiles,
            remote,
            resolved_api_key,
            resolved_base_url,
        ) {
            created_profiles += 1;
        } else {
            updated_profiles += 1;
        }
    }

    save_model_profiles(&paths, &local_profiles)?;

    Ok(RemoteAuthSyncResult {
        total_remote_profiles: remote_profiles.len(),
        synced_profiles: created_profiles + updated_profiles,
        created_profiles,
        updated_profiles,
        resolved_keys,
        unresolved_keys,
        failed_key_resolves,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(
        id: &str,
        provider: &str,
        model: &str,
        auth_ref: &str,
        api_key: Option<&str>,
    ) -> ModelProfile {
        ModelProfile {
            id: id.to_string(),
            name: format!("{provider}/{model}"),
            provider: provider.to_string(),
            model: model.to_string(),
            auth_ref: auth_ref.to_string(),
            api_key: api_key.map(|v| v.to_string()),
            base_url: None,
            description: None,
            enabled: true,
        }
    }

    #[test]
    fn merge_remote_profile_reuses_local_entry_by_provider_model() {
        let mut local = vec![profile(
            "local-1",
            "anthropic",
            "claude-4-5",
            "anthropic:default",
            Some("local-key"),
        )];
        let remote = profile(
            "remote-9",
            "anthropic",
            "claude-4-5",
            "anthropic:remote",
            None,
        );

        let created = merge_remote_profile_into_local(&mut local, &remote, None, None);

        assert!(!created);
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].id, "local-1");
        assert_eq!(local[0].api_key.as_deref(), Some("local-key"));
        assert_eq!(local[0].auth_ref, "anthropic:default");
    }

    #[test]
    fn merge_remote_profile_fills_missing_local_key_from_resolved_remote() {
        let mut local = vec![profile(
            "local-2",
            "openai",
            "gpt-4.1",
            "openai:default",
            None,
        )];
        let remote = profile("remote-2", "openai", "gpt-4.1", "openai:default", None);

        let created = merge_remote_profile_into_local(
            &mut local,
            &remote,
            Some("resolved-remote-key".to_string()),
            None,
        );

        assert!(!created);
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].api_key.as_deref(), Some("resolved-remote-key"));
    }

    #[test]
    fn merge_remote_profile_prefers_resolved_key_over_stale_remote_key() {
        let mut local = vec![profile(
            "local-3",
            "anthropic",
            "claude-4-5",
            "anthropic:default",
            None,
        )];
        let remote = profile(
            "remote-3",
            "anthropic",
            "claude-4-5",
            "anthropic:default",
            Some("stale-remote-key"),
        );

        let created = merge_remote_profile_into_local(
            &mut local,
            &remote,
            Some("resolved-valid-key".to_string()),
            None,
        );

        assert!(!created);
        assert_eq!(local[0].api_key.as_deref(), Some("resolved-valid-key"));
    }

    #[test]
    fn dedupe_profiles_prefers_entry_with_api_key() {
        let weak = profile("weak", "anthropic", "claude-4-5", "", None);
        let strong = profile(
            "strong",
            "anthropic",
            "claude-4-5",
            "anthropic:default",
            Some("k-123"),
        );

        let deduped = dedupe_profiles_by_model_key(vec![weak, strong]);

        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].id, "strong");
        assert_eq!(deduped[0].api_key.as_deref(), Some("k-123"));
    }
}

#[tauri::command]
pub fn get_cached_model_catalog() -> Result<Vec<ModelCatalogProvider>, String> {
    let paths = resolve_paths();
    let cache_path = model_catalog_cache_path(&paths);
    let current_version = resolve_openclaw_version();
    if let Some(catalog) = select_catalog_from_cache(
        read_model_catalog_cache(&cache_path).as_ref(),
        &current_version,
    ) {
        return Ok(catalog);
    }
    Ok(Vec::new())
}

#[tauri::command]
pub fn refresh_model_catalog() -> Result<Vec<ModelCatalogProvider>, String> {
    let paths = resolve_paths();
    load_model_catalog(&paths)
}

#[tauri::command]
pub fn list_model_profiles() -> Result<Vec<ModelProfile>, String> {
    let openclaw = clawpal_core::openclaw::OpenclawCli::new();
    clawpal_core::profile::list_profiles(&openclaw).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn extract_model_profiles_from_config() -> Result<ExtractModelProfilesResult, String> {
    let paths = resolve_paths();
    let cfg = read_openclaw_config(&paths)?;
    let profiles = load_model_profiles(&paths);
    let bindings = collect_model_bindings(&cfg, &profiles);
    let mut created = 0usize;
    let mut reused = 0usize;
    let mut skipped_invalid = 0usize;
    let mut seen = HashSet::new();

    let mut next_profiles = profiles;
    let mut model_profile_map: HashMap<String, String> = HashMap::new();
    for profile in &next_profiles {
        model_profile_map.insert(
            normalize_model_ref(&profile_to_model_value(profile)),
            profile.id.clone(),
        );
    }

    for binding in bindings {
        let scope_label = match binding.scope.as_str() {
            "global" => "global".to_string(),
            "agent" => format!("agent:{}", binding.scope_id),
            "channel" => format!("channel:{}", binding.scope_id),
            _ => binding.scope_id,
        };
        let Some(model_ref) = binding.model_value else {
            continue;
        };
        let model_ref = normalize_model_ref(&model_ref);
        if model_ref.trim().is_empty() {
            continue;
        }
        if model_profile_map.contains_key(&model_ref) || seen.contains(&model_ref) {
            reused += 1;
            continue;
        }
        let mut parts = model_ref.splitn(2, '/');
        let provider = parts.next().unwrap_or("").trim();
        let model = parts.next().unwrap_or("").trim();
        if provider.is_empty() || model.is_empty() {
            skipped_invalid += 1;
            continue;
        }
        let auth_ref = resolve_auth_ref_for_provider(&cfg, provider)
            .unwrap_or_else(|| format!("{provider}:default"));
        let base_url = resolve_model_provider_base_url(&cfg, provider);
        let profile = ModelProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("{scope_label} model profile"),
            provider: provider.to_string(),
            model: model.to_string(),
            auth_ref,
            api_key: None,
            base_url,
            description: Some(format!("Extracted from config ({scope_label})")),
            enabled: true,
        };
        let key = profile_to_model_value(&profile);
        model_profile_map.insert(normalize_model_ref(&key), profile.id.clone());
        next_profiles.push(profile);
        seen.insert(model_ref);
        created += 1;
    }

    if created > 0 {
        save_model_profiles(&paths, &next_profiles)?;
    }

    Ok(ExtractModelProfilesResult {
        created,
        reused,
        skipped_invalid,
    })
}

#[tauri::command]
pub fn upsert_model_profile(profile: ModelProfile) -> Result<ModelProfile, String> {
    let paths = resolve_paths();
    let path = model_profiles_path(&paths);
    let content = std::fs::read_to_string(&path).unwrap_or_else(|_| r#"{"profiles":[]}"#.into());
    let (saved, next_json) =
        clawpal_core::profile::upsert_profile_in_storage_json(&content, profile)
            .map_err(|e| e.to_string())?;
    crate::config_io::write_text(&path, &next_json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(saved)
}

#[tauri::command]
pub fn delete_model_profile(profile_id: String) -> Result<bool, String> {
    let openclaw = clawpal_core::openclaw::OpenclawCli::new();
    clawpal_core::profile::delete_profile(&openclaw, &profile_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resolve_provider_auth(provider: String) -> Result<ProviderAuthSuggestion, String> {
    let provider_trimmed = provider.trim();
    if provider_trimmed.is_empty() {
        return Ok(ProviderAuthSuggestion {
            auth_ref: None,
            has_key: false,
            source: String::new(),
        });
    }
    let paths = resolve_paths();
    let cfg = read_openclaw_config(&paths)?;
    let global_base = local_global_openclaw_base_dir();

    // 1. Check openclaw config auth profiles
    if let Some(auth_ref) = resolve_auth_ref_for_provider(&cfg, provider_trimmed) {
        let probe_profile = ModelProfile {
            id: "provider-auth-probe".into(),
            name: "provider-auth-probe".into(),
            provider: provider_trimmed.to_string(),
            model: "probe".into(),
            auth_ref: auth_ref.clone(),
            api_key: None,
            base_url: None,
            description: None,
            enabled: true,
        };
        let key = resolve_profile_api_key(&probe_profile, &global_base);
        if !key.trim().is_empty() {
            return Ok(ProviderAuthSuggestion {
                auth_ref: Some(auth_ref),
                has_key: true,
                source: "openclaw auth profile".into(),
            });
        }
    }

    // 2. Check env vars
    for env_name in provider_env_var_candidates(provider_trimmed) {
        if std::env::var(&env_name)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
        {
            return Ok(ProviderAuthSuggestion {
                auth_ref: Some(env_name),
                has_key: true,
                source: "environment variable".into(),
            });
        }
    }

    // 3. Check existing model profiles for this provider
    let profiles = load_model_profiles(&paths);
    for p in &profiles {
        if p.provider.eq_ignore_ascii_case(provider_trimmed) {
            let key = resolve_profile_api_key(p, &global_base);
            if !key.is_empty() {
                let auth_ref = if !p.auth_ref.trim().is_empty() {
                    Some(p.auth_ref.clone())
                } else {
                    None
                };
                return Ok(ProviderAuthSuggestion {
                    auth_ref,
                    has_key: true,
                    source: format!("existing profile {}/{}", p.provider, p.model),
                });
            }
        }
    }

    Ok(ProviderAuthSuggestion {
        auth_ref: None,
        has_key: false,
        source: String::new(),
    })
}

#[tauri::command]
pub fn resolve_api_keys() -> Result<Vec<ResolvedApiKey>, String> {
    let paths = resolve_paths();
    let profiles = load_model_profiles(&paths);
    let global_base = local_global_openclaw_base_dir();
    let mut out = Vec::new();
    for profile in &profiles {
        let key = resolve_profile_api_key(profile, &global_base);
        let masked = mask_api_key(&key);
        out.push(ResolvedApiKey {
            profile_id: profile.id.clone(),
            masked_key: masked,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn test_model_profile(profile_id: String) -> Result<bool, String> {
    let paths = resolve_paths();
    let profiles = load_model_profiles(&paths);
    let profile = profiles
        .into_iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| format!("Profile not found: {profile_id}"))?;

    if !profile.enabled {
        return Err("Profile is disabled".into());
    }

    let global_base = local_global_openclaw_base_dir();
    let api_key = resolve_profile_api_key(&profile, &global_base);
    if api_key.trim().is_empty() && !provider_supports_optional_api_key(&profile.provider) {
        let provider = profile.provider.trim().to_ascii_lowercase();
        let hint = if provider == "anthropic" {
            " For Claude setup-token, also try exporting ANTHROPIC_OAUTH_TOKEN or ANTHROPIC_AUTH_TOKEN."
        } else {
            ""
        };
        return Err(
            format!("No API key resolved for this profile. Set apiKey directly, configure auth_ref in auth store (auth-profiles.json/auth.json), or export auth_ref on local shell.{hint}"),
        );
    }

    let resolved_base_url = profile
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .or_else(|| {
            read_openclaw_config(&paths)
                .ok()
                .and_then(|cfg| resolve_model_provider_base_url(&cfg, &profile.provider))
        });

    tauri::async_runtime::spawn_blocking(move || {
        run_provider_probe(profile.provider, profile.model, resolved_base_url, api_key)
    })
    .await
    .map_err(|e| format!("Task join failed: {e}"))??;

    Ok(true)
}

#[tauri::command]
pub async fn remote_refresh_model_catalog(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<ModelCatalogProvider>, String> {
    let paths = resolve_paths();
    let cache_path = remote_model_catalog_cache_path(&paths, &host_id);
    let remote_version = match pool.exec_login(&host_id, "openclaw --version").await {
        Ok(r) => {
            extract_version_from_text(&r.stdout).unwrap_or_else(|| r.stdout.trim().to_string())
        }
        Err(_) => "unknown".into(),
    };
    let cached = read_model_catalog_cache(&cache_path);
    if let Some(selected) = select_catalog_from_cache(cached.as_ref(), &remote_version) {
        return Ok(selected);
    }

    let result = pool
        .exec_login(&host_id, "openclaw models list --all --json --no-color")
        .await;
    if let Ok(r) = result {
        if r.exit_code == 0 && !r.stdout.trim().is_empty() {
            if let Some(catalog) = parse_model_catalog_from_cli_output(&r.stdout) {
                let cache = ModelCatalogProviderCache {
                    cli_version: remote_version,
                    updated_at: unix_timestamp_secs(),
                    providers: catalog.clone(),
                    source: "openclaw models list --all --json".into(),
                    error: None,
                };
                let _ = save_model_catalog_cache(&cache_path, &cache);
                return Ok(catalog);
            }
        }
    }
    if let Some(previous) = cached {
        if !previous.providers.is_empty() && previous.error.is_none() {
            return Ok(previous.providers);
        }
    }
    Err("Failed to load remote model catalog from openclaw CLI".into())
}
