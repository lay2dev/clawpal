use super::*;

#[tauri::command]
pub async fn remote_list_model_profiles(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<ModelProfile>, String> {
    let content = pool
        .sftp_read(&host_id, "~/.clawpal/model-profiles.json")
        .await
        .unwrap_or_else(|_| r#"{"profiles":[]}"#.to_string());
    Ok(clawpal_core::profile::list_profiles_from_storage_json(&content))
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
    let (saved, next_json) = clawpal_core::profile::upsert_profile_in_storage_json(&content, profile)
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
    if api_key.trim().is_empty() {
        return Err(
            "No API key resolved for this remote profile. Set apiKey directly, configure auth_ref in remote auth-profiles.json, or export auth_ref on remote shell.".into(),
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
    let (_config_path, _raw, cfg) = remote_read_openclaw_config_text_and_json(&pool, &host_id).await?;

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
pub fn upsert_model_profile(mut profile: ModelProfile) -> Result<ModelProfile, String> {
    let openclaw = clawpal_core::openclaw::OpenclawCli::new();
    profile = clawpal_core::profile::upsert_profile(&openclaw, profile)
        .map_err(|e| e.to_string())?;
    Ok(profile)
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

    // 1. Check openclaw config auth profiles
    if let Some(auth_ref) = resolve_auth_ref_for_provider(&cfg, provider_trimmed) {
        return Ok(ProviderAuthSuggestion {
            auth_ref: Some(auth_ref),
            has_key: true,
            source: "openclaw auth profile".into(),
        });
    }

    // 2. Check env vars
    let provider_upper = provider_trimmed.to_uppercase().replace('-', "_");
    for suffix in ["_API_KEY", "_KEY", "_TOKEN"] {
        let env_name = format!("{provider_upper}{suffix}");
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
    let global_base = global_profile_base_dir();
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
    let global_base = global_profile_base_dir();
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
    let openclaw = clawpal_core::openclaw::OpenclawCli::new();
    let result =
        clawpal_core::profile::test_profile(&openclaw, &profile_id).map_err(|e| e.to_string())?;
    Ok(result.ok)
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
