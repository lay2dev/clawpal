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
}

/// List current channel→agent bindings from config.
#[tauri::command]
pub fn delete_channel_node(path: String) -> Result<bool, String> {
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
}

#[tauri::command]
pub fn set_global_model(model_value: Option<String>) -> Result<bool, String> {
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
}

#[tauri::command]
pub fn set_agent_model(agent_id: String, model_value: Option<String>) -> Result<bool, String> {
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
}

#[tauri::command]
pub fn set_channel_model(path: String, model_value: Option<String>) -> Result<bool, String> {
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
}

#[tauri::command]
pub fn list_model_bindings() -> Result<Vec<ModelBinding>, String> {
    let paths = resolve_paths();
    let cfg = read_openclaw_config(&paths)?;
    let profiles = load_model_profiles(&paths);
    Ok(collect_model_bindings(&cfg, &profiles))
}
