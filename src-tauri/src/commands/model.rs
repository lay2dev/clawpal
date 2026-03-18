use super::*;

/// Resolve Discord guild/channel names via openclaw CLI and persist to cache.
#[tauri::command]
pub fn update_channel_config(
    path: String,
    channel_type: Option<String>,
    mode: Option<String>,
    allowlist: Vec<String>,
    model: Option<String>,
) -> Result<bool, String> {
    timed_sync!("update_channel_config", {
        if path.trim().is_empty() {
            return Err("channel path is required".into());
        }
        let paths = resolve_paths();
        let mut cfg = read_openclaw_config(&paths)?;
        let current = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
        set_nested_value(
            &mut cfg,
            &format!("{path}.type"),
            channel_type.map(Value::String),
        )?;
        set_nested_value(&mut cfg, &format!("{path}.mode"), mode.map(Value::String))?;
        let allowlist_values = allowlist.into_iter().map(Value::String).collect::<Vec<_>>();
        set_nested_value(
            &mut cfg,
            &format!("{path}.allowlist"),
            Some(Value::Array(allowlist_values)),
        )?;
        set_nested_value(&mut cfg, &format!("{path}.model"), model.map(Value::String))?;
        write_config_with_snapshot(&paths, &current, &cfg, "update-channel")?;
        Ok(true)
    })
}

/// List current channel→agent bindings from config.
#[tauri::command]
pub fn delete_channel_node(path: String) -> Result<bool, String> {
    timed_sync!("delete_channel_node", {
        if path.trim().is_empty() {
            return Err("channel path is required".into());
        }
        let paths = resolve_paths();
        let mut cfg = read_openclaw_config(&paths)?;
        let current = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
        let before = cfg.to_string();
        set_nested_value(&mut cfg, &path, None)?;
        if cfg.to_string() == before {
            return Ok(false);
        }
        write_config_with_snapshot(&paths, &current, &cfg, "delete-channel")?;
        Ok(true)
    })
}

#[tauri::command]
pub fn set_global_model(model_value: Option<String>) -> Result<bool, String> {
    timed_sync!("set_global_model", {
        let paths = resolve_paths();
        let mut cfg = read_openclaw_config(&paths)?;
        let current = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
        let model = model_value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        // If existing model is an object (has fallbacks etc.), only update "primary" inside it
        if let Some(existing) = cfg.pointer_mut("/agents/defaults/model") {
            if let Some(model_obj) = existing.as_object_mut() {
                let sync_model_value = match model.clone() {
                    Some(v) => {
                        model_obj.insert("primary".into(), Value::String(v.clone()));
                        Some(v)
                    }
                    None => {
                        model_obj.remove("primary");
                        None
                    }
                };
                write_config_with_snapshot(&paths, &current, &cfg, "set-global-model")?;
                maybe_sync_main_auth_for_model_value(&paths, sync_model_value)?;
                return Ok(true);
            }
        }
        // Fallback: plain string or missing — set the whole value
        set_nested_value(&mut cfg, "agents.defaults.model", model.map(Value::String))?;
        write_config_with_snapshot(&paths, &current, &cfg, "set-global-model")?;
        let model_to_sync = cfg
            .pointer("/agents/defaults/model")
            .and_then(read_model_value);
        maybe_sync_main_auth_for_model_value(&paths, model_to_sync)?;
        Ok(true)
    })
}

#[tauri::command]
pub fn set_agent_model(agent_id: String, model_value: Option<String>) -> Result<bool, String> {
    timed_sync!("set_agent_model", {
        if agent_id.trim().is_empty() {
            return Err("agent id is required".into());
        }
        let paths = resolve_paths();
        let mut cfg = read_openclaw_config(&paths)?;
        let current = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
        let value = model_value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        set_agent_model_value(&mut cfg, &agent_id, value)?;
        write_config_with_snapshot(&paths, &current, &cfg, "set-agent-model")?;
        Ok(true)
    })
}

#[tauri::command]
pub fn set_channel_model(path: String, model_value: Option<String>) -> Result<bool, String> {
    timed_sync!("set_channel_model", {
        if path.trim().is_empty() {
            return Err("channel path is required".into());
        }
        let paths = resolve_paths();
        let mut cfg = read_openclaw_config(&paths)?;
        let current = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
        let value = model_value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        set_nested_value(&mut cfg, &format!("{path}.model"), value.map(Value::String))?;
        write_config_with_snapshot(&paths, &current, &cfg, "set-channel-model")?;
        Ok(true)
    })
}

#[tauri::command]
pub fn list_model_bindings() -> Result<Vec<ModelBinding>, String> {
    timed_sync!("list_model_bindings", {
        let paths = resolve_paths();
        let cfg = read_openclaw_config(&paths)?;
        let profiles = load_model_profiles(&paths);
        Ok(collect_model_bindings(&cfg, &profiles))
    })
}

// --- Extracted from mod.rs ---

pub(crate) fn read_model_catalog_cache(path: &Path) -> Option<ModelCatalogProviderCache> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<ModelCatalogProviderCache>(&text).ok()
}

pub(crate) fn save_model_catalog_cache(
    path: &Path,
    cache: &ModelCatalogProviderCache,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(cache).map_err(|error| error.to_string())?;
    write_text(path, &text)
}

pub(crate) fn model_catalog_cache_path(paths: &crate::models::OpenClawPaths) -> PathBuf {
    paths.clawpal_dir.join("model-catalog-cache.json")
}

pub(crate) fn remote_model_catalog_cache_path(
    paths: &crate::models::OpenClawPaths,
    host_id: &str,
) -> PathBuf {
    let safe_host_id: String = host_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    paths
        .clawpal_dir
        .join("remote-model-catalog")
        .join(format!("{safe_host_id}.json"))
}

pub(crate) fn normalize_model_ref(raw: &str) -> String {
    raw.trim().to_lowercase().replace('\\', "/")
}

pub(crate) fn collect_model_summary(cfg: &Value) -> ModelSummary {
    let global_default_model = cfg
        .pointer("/agents/defaults/model")
        .and_then(|value| read_model_value(value))
        .or_else(|| {
            cfg.pointer("/agents/default/model")
                .and_then(|value| read_model_value(value))
        });

    let mut agent_overrides = Vec::new();
    if let Some(agents) = cfg.pointer("/agents/list").and_then(Value::as_array) {
        for agent in agents {
            if let Some(model_value) = agent.get("model").and_then(read_model_value) {
                let should_emit = global_default_model
                    .as_ref()
                    .map(|global| global != &model_value)
                    .unwrap_or(true);
                if should_emit {
                    let id = agent.get("id").and_then(Value::as_str).unwrap_or("agent");
                    agent_overrides.push(format!("{id} => {model_value}"));
                }
            }
        }
    }
    ModelSummary {
        global_default_model,
        agent_overrides,
        channel_overrides: collect_channel_model_overrides(cfg),
    }
}

pub(crate) fn collect_main_auth_model_candidates(cfg: &Value) -> Vec<String> {
    let mut models = Vec::new();
    if let Some(model) = cfg
        .pointer("/agents/defaults/model")
        .and_then(read_model_value)
    {
        models.push(model);
    }
    if let Some(agents) = cfg.pointer("/agents/list").and_then(Value::as_array) {
        for agent in agents {
            let is_main = agent
                .get("id")
                .and_then(Value::as_str)
                .map(|id| id.eq_ignore_ascii_case("main"))
                .unwrap_or(false);
            if !is_main {
                continue;
            }
            if let Some(model) = agent.get("model").and_then(read_model_value) {
                models.push(model);
            }
        }
    }
    models
}

pub(crate) fn load_model_catalog(
    paths: &crate::models::OpenClawPaths,
) -> Result<Vec<ModelCatalogProvider>, String> {
    let cache_path = model_catalog_cache_path(paths);
    let current_version = resolve_openclaw_version();
    let cached = read_model_catalog_cache(&cache_path);
    if let Some(selected) = select_catalog_from_cache(cached.as_ref(), &current_version) {
        return Ok(selected);
    }

    if let Some(catalog) = extract_model_catalog_from_cli(paths) {
        if !catalog.is_empty() {
            return Ok(catalog);
        }
    }

    if let Some(previous) = cached {
        if !previous.providers.is_empty() && previous.error.is_none() {
            return Ok(previous.providers);
        }
    }

    Err("Failed to load model catalog from openclaw CLI".into())
}

pub(crate) fn select_catalog_from_cache(
    cached: Option<&ModelCatalogProviderCache>,
    current_version: &str,
) -> Option<Vec<ModelCatalogProvider>> {
    let cache = cached?;
    if cache.cli_version != current_version {
        return None;
    }
    if cache.error.is_some() || cache.providers.is_empty() {
        return None;
    }
    Some(cache.providers.clone())
}

/// Parse CLI output from `openclaw models list --all --json` into grouped providers.
/// Handles various output formats: flat arrays, {models: [...]}, {items: [...]}, {data: [...]}.
/// Strips prefix junk (plugin log lines) before the JSON.
pub(crate) fn parse_model_catalog_from_cli_output(raw: &str) -> Option<Vec<ModelCatalogProvider>> {
    let json_str = clawpal_core::doctor::extract_json_from_output(raw)?;
    let response: Value = serde_json::from_str(json_str).ok()?;
    let models: Vec<Value> = response
        .as_array()
        .map(|values| values.to_vec())
        .or_else(|| {
            response
                .get("models")
                .and_then(Value::as_array)
                .map(|values| values.to_vec())
        })
        .or_else(|| {
            response
                .get("items")
                .and_then(Value::as_array)
                .map(|values| values.to_vec())
        })
        .or_else(|| {
            response
                .get("data")
                .and_then(Value::as_array)
                .map(|values| values.to_vec())
        })
        .unwrap_or_default();
    if models.is_empty() {
        return None;
    }
    let mut providers: BTreeMap<String, ModelCatalogProvider> = BTreeMap::new();
    for model in &models {
        let key = model
            .get("key")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                let provider = model.get("provider").and_then(Value::as_str)?;
                let model_id = model.get("id").and_then(Value::as_str)?;
                Some(format!("{provider}/{model_id}"))
            });
        let key = match key {
            Some(k) => k,
            None => continue,
        };
        let mut parts = key.splitn(2, '/');
        let provider = match parts.next() {
            Some(p) if !p.trim().is_empty() => p.trim().to_lowercase(),
            _ => continue,
        };
        let id = parts.next().unwrap_or("").trim().to_string();
        if id.is_empty() {
            continue;
        }
        let name = model
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| model.get("model").and_then(Value::as_str))
            .or_else(|| model.get("title").and_then(Value::as_str))
            .map(str::to_string);
        let base_url = model
            .get("baseUrl")
            .or_else(|| model.get("base_url"))
            .or_else(|| model.get("apiBase"))
            .or_else(|| model.get("api_base"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                response
                    .get("providers")
                    .and_then(Value::as_object)
                    .and_then(|providers| providers.get(&provider))
                    .and_then(Value::as_object)
                    .and_then(|provider_cfg| {
                        provider_cfg
                            .get("baseUrl")
                            .or_else(|| provider_cfg.get("base_url"))
                            .or_else(|| provider_cfg.get("apiBase"))
                            .or_else(|| provider_cfg.get("api_base"))
                            .and_then(Value::as_str)
                    })
                    .map(str::to_string)
            });
        let entry = providers
            .entry(provider.clone())
            .or_insert(ModelCatalogProvider {
                provider: provider.clone(),
                base_url,
                models: Vec::new(),
            });
        if !entry.models.iter().any(|existing| existing.id == id) {
            entry.models.push(ModelCatalogModel {
                id: id.clone(),
                name: name.clone(),
            });
        }
    }

    if providers.is_empty() {
        return None;
    }

    let mut out: Vec<ModelCatalogProvider> = providers.into_values().collect();
    for provider in &mut out {
        provider.models.sort_by(|a, b| a.id.cmp(&b.id));
    }
    out.sort_by(|a, b| a.provider.cmp(&b.provider));
    Some(out)
}

pub(crate) fn extract_model_catalog_from_cli(
    paths: &crate::models::OpenClawPaths,
) -> Option<Vec<ModelCatalogProvider>> {
    let output = run_openclaw_raw(&["models", "list", "--all", "--json", "--no-color"]).ok()?;
    if output.stdout.trim().is_empty() {
        return None;
    }

    let out = parse_model_catalog_from_cli_output(&output.stdout)?;
    let _ = cache_model_catalog(paths, out.clone());
    Some(out)
}

pub(crate) fn cache_model_catalog(
    paths: &crate::models::OpenClawPaths,
    providers: Vec<ModelCatalogProvider>,
) -> Option<()> {
    let cache_path = model_catalog_cache_path(paths);
    let now = unix_timestamp_secs();
    let cache = ModelCatalogProviderCache {
        cli_version: resolve_openclaw_version(),
        updated_at: now,
        providers,
        source: "openclaw models list --all --json".into(),
        error: None,
    };
    let _ = save_model_catalog_cache(&cache_path, &cache);
    Some(())
}

#[cfg(test)]
mod model_catalog_cache_tests {
    use super::*;

    #[test]
    pub(crate) fn test_select_cached_catalog_same_version() {
        let cached = ModelCatalogProviderCache {
            cli_version: "1.2.3".into(),
            updated_at: 123,
            providers: vec![ModelCatalogProvider {
                provider: "openrouter".into(),
                base_url: None,
                models: vec![ModelCatalogModel {
                    id: "moonshotai/kimi-k2.5".into(),
                    name: Some("Kimi".into()),
                }],
            }],
            source: "openclaw models list --all --json".into(),
            error: None,
        };
        let selected = select_catalog_from_cache(Some(&cached), "1.2.3");
        assert!(selected.is_some(), "same version should use cache");
    }

    #[test]
    pub(crate) fn test_select_cached_catalog_version_mismatch_requires_refresh() {
        let cached = ModelCatalogProviderCache {
            cli_version: "1.2.2".into(),
            updated_at: 123,
            providers: vec![ModelCatalogProvider {
                provider: "openrouter".into(),
                base_url: None,
                models: vec![ModelCatalogModel {
                    id: "moonshotai/kimi-k2.5".into(),
                    name: Some("Kimi".into()),
                }],
            }],
            source: "openclaw models list --all --json".into(),
            error: None,
        };
        let selected = select_catalog_from_cache(Some(&cached), "1.2.3");
        assert!(
            selected.is_none(),
            "version mismatch must force CLI refresh"
        );
    }
}

#[cfg(test)]
mod model_value_tests {
    use super::*;

    pub(crate) fn profile(provider: &str, model: &str) -> ModelProfile {
        ModelProfile {
            id: "p1".into(),
            name: "p".into(),
            provider: provider.into(),
            model: model.into(),
            auth_ref: "".into(),
            api_key: None,
            base_url: None,
            description: None,
            enabled: true,
        }
    }

    #[test]
    pub(crate) fn test_profile_to_model_value_keeps_provider_prefix_for_nested_model_id() {
        let p = profile("openrouter", "moonshotai/kimi-k2.5");
        assert_eq!(
            profile_to_model_value(&p),
            "openrouter/moonshotai/kimi-k2.5",
        );
    }

    #[test]
    pub(crate) fn test_default_base_url_supports_openai_codex_family() {
        assert_eq!(
            default_base_url_for_provider("openai-codex"),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(
            default_base_url_for_provider("github-copilot"),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(
            default_base_url_for_provider("copilot"),
            Some("https://api.openai.com/v1")
        );
    }
}

pub(crate) fn collect_model_bindings(cfg: &Value, profiles: &[ModelProfile]) -> Vec<ModelBinding> {
    let mut out = Vec::new();
    let global = cfg
        .pointer("/agents/defaults/model")
        .or_else(|| cfg.pointer("/agents/default/model"))
        .and_then(read_model_value);
    out.push(ModelBinding {
        scope: "global".into(),
        scope_id: "global".into(),
        model_profile_id: find_profile_by_model(profiles, global.as_deref()),
        model_value: global,
        path: Some("agents.defaults.model".into()),
    });

    if let Some(agents) = cfg
        .get("agents")
        .and_then(|v| v.get("list"))
        .and_then(Value::as_array)
    {
        for agent in agents {
            let id = agent.get("id").and_then(Value::as_str).unwrap_or("agent");
            let model = agent.get("model").and_then(read_model_value);
            out.push(ModelBinding {
                scope: "agent".into(),
                scope_id: id.to_string(),
                model_profile_id: find_profile_by_model(profiles, model.as_deref()),
                model_value: model,
                path: Some(format!("agents.list.{id}.model")),
            });
        }
    }

    pub(crate) fn walk_channel_binding(
        prefix: &str,
        node: &Value,
        out: &mut Vec<ModelBinding>,
        profiles: &[ModelProfile],
    ) {
        if let Some(obj) = node.as_object() {
            if let Some(model) = obj.get("model").and_then(read_model_value) {
                out.push(ModelBinding {
                    scope: "channel".into(),
                    scope_id: prefix.to_string(),
                    model_profile_id: find_profile_by_model(profiles, Some(&model)),
                    model_value: Some(model),
                    path: Some(format!("{}.model", prefix)),
                });
            }
            for (k, child) in obj {
                if let Value::Object(_) = child {
                    walk_channel_binding(&format!("{}.{}", prefix, k), child, out, profiles);
                }
            }
        }
    }

    if let Some(channels) = cfg.get("channels") {
        walk_channel_binding("channels", channels, &mut out, profiles);
    }

    out
}

pub(crate) fn find_profile_by_model(
    profiles: &[ModelProfile],
    value: Option<&str>,
) -> Option<String> {
    let value = value?;
    let normalized = normalize_model_ref(value);
    for profile in profiles {
        if normalize_model_ref(&profile_to_model_value(profile)) == normalized
            || normalize_model_ref(&profile.model) == normalized
        {
            return Some(profile.id.clone());
        }
    }
    None
}

pub(crate) fn resolve_auth_ref_for_provider(cfg: &Value, provider: &str) -> Option<String> {
    let provider = provider.trim().to_lowercase();
    if provider.is_empty() {
        return None;
    }
    if let Some(auth_profiles) = cfg.pointer("/auth/profiles").and_then(Value::as_object) {
        let mut fallback = None;
        for (profile_id, profile) in auth_profiles {
            let entry_provider = profile.get("provider").or_else(|| profile.get("name"));
            if let Some(entry_provider) = entry_provider.and_then(Value::as_str) {
                if entry_provider.trim().eq_ignore_ascii_case(&provider) {
                    if profile_id.ends_with(":default") {
                        return Some(profile_id.clone());
                    }
                    if fallback.is_none() {
                        fallback = Some(profile_id.clone());
                    }
                }
            }
        }
        if fallback.is_some() {
            return fallback;
        }
    }
    None
}

pub(crate) fn resolve_model_provider_base_url(cfg: &Value, provider: &str) -> Option<String> {
    let provider = provider.trim();
    if provider.is_empty() {
        return None;
    }
    cfg.pointer("/models/providers")
        .and_then(Value::as_object)
        .and_then(|providers| providers.get(provider))
        .and_then(Value::as_object)
        .and_then(|provider_cfg| {
            provider_cfg
                .get("baseUrl")
                .or_else(|| provider_cfg.get("base_url"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    provider_cfg
                        .get("apiBase")
                        .or_else(|| provider_cfg.get("api_base"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
        })
}
