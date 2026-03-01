//! Config domain: pure JSON manipulation functions for openclaw.json
//!
//! This module provides string-in/string-out functions for config operations.
//! All I/O (SFTP, SSH) is handled by the Tauri layer.

use serde_json::{json, Value};

/// Parse raw config text and return (parsed_json, pretty_printed_text)
///
/// The returned text is normalized pretty-printed form suitable for snapshot/storage.
pub fn parse_and_normalize_config(raw: &str) -> Result<(Value, String), String> {
    let parsed = crate::doctor::parse_json_document(raw, "config")?;
    let normalized = crate::doctor::render_json_document(&parsed, "config")?;
    Ok((parsed, normalized))
}

/// Parse raw config using JSON5 (allows trailing commas, comments)
pub fn parse_config_json5(raw: &str) -> Value {
    crate::doctor::parse_json5_document_or_default(raw)
}

/// Prepare a config write operation with snapshot
///
/// Returns the new config text and the snapshot text (current config).
/// The caller is responsible for:
///   1. Writing the snapshot to ~/.clawpal/snapshots/{ts}-{source}.json
///   2. Writing the new config to the config path
pub fn prepare_config_write(
    current_raw: &str,
    next: &Value,
    _source: &str,
) -> Result<(String, String), String> {
    // Normalize current for snapshot
    let snapshot_text = crate::doctor::render_json_document(
        &crate::doctor::parse_json5_document_or_default(current_raw),
        "config",
    )?;

    // Serialize new config
    let new_text = crate::doctor::render_json_document(next, "remote config")?;

    Ok((new_text, snapshot_text))
}

/// Build candidate config from a recipe template
///
/// Returns (candidate_config, change_paths)
pub fn build_candidate_config(
    current: &Value,
    patch_template: &str,
    params: &serde_json::Map<String, Value>,
) -> Result<(Value, Vec<String>), String> {
    // Start from current config
    let mut candidate = current.clone();

    // Apply template patches
    apply_template_patches(&mut candidate, patch_template, params)?;

    // Collect change paths
    let changes = collect_change_paths(current, &candidate);

    Ok((candidate, changes))
}

/// Apply template-based patches to config
fn apply_template_patches(
    config: &mut Value,
    template: &str,
    params: &serde_json::Map<String, Value>,
) -> Result<(), String> {
    match template {
        "set-gateway-port" => {
            let port = params
                .get("port")
                .and_then(Value::as_u64)
                .ok_or("port parameter required")?;
            crate::doctor::upsert_json_path(
                config,
                "gateway.port",
                Value::Number(serde_json::Number::from(port)),
            )?;
        }
        "set-global-model" => {
            let model = params
                .get("model")
                .and_then(Value::as_str)
                .ok_or("model parameter required")?;
            crate::doctor::upsert_json_path(config, "agents.defaults.model", json!(model))?;
        }
        "set-agent-model" => {
            let agent_id = params
                .get("agentId")
                .and_then(Value::as_str)
                .ok_or("agentId parameter required")?;
            let model = params
                .get("model")
                .and_then(Value::as_str)
                .ok_or("model parameter required")?;
            let path = format!("agents.list.{agent_id}.model");
            crate::doctor::upsert_json_path(config, &path, json!(model))?;
        }
        "enable-channel" => {
            let channel_path = params
                .get("channelPath")
                .and_then(Value::as_str)
                .ok_or("channelPath parameter required")?;
            let path = format!("{channel_path}.enabled");
            crate::doctor::upsert_json_path(config, &path, json!(true))?;
        }
        "disable-channel" => {
            let channel_path = params
                .get("channelPath")
                .and_then(Value::as_str)
                .ok_or("channelPath parameter required")?;
            let path = format!("{channel_path}.enabled");
            crate::doctor::upsert_json_path(config, &path, json!(false))?;
        }
        "delete-channel" => {
            let channel_path = params
                .get("channelPath")
                .and_then(Value::as_str)
                .ok_or("channelPath parameter required")?;
            crate::doctor::delete_json_path(config, channel_path);
        }
        "create-agent" => {
            let agent_id = params
                .get("agentId")
                .and_then(Value::as_str)
                .ok_or("agentId parameter required")?;
            let model = params.get("model").and_then(Value::as_str);
            let independent = params
                .get("independent")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let mut agent_obj = serde_json::json!({
                "id": agent_id
            });
            if let Some(m) = model {
                agent_obj["model"] = json!(m);
            }
            if independent {
                // Note: workspace path is platform-specific, caller handles it
                agent_obj["workspace"] = json!(format!("~/.openclaw/workspaces/{agent_id}"));
            }

            // Ensure agents.list exists
            if crate::doctor::json_path_get(config, "agents.list").is_none() {
                crate::doctor::upsert_json_path(config, "agents", json!({"list": []}))?;
            }

            // Add agent to list
            if let Some(list) = config
                .pointer_mut("/agents/list")
                .and_then(Value::as_array_mut)
            {
                // Remove existing agent with same id
                list.retain(|a| a.get("id").and_then(Value::as_str) != Some(agent_id));
                list.push(agent_obj);
            }
        }
        "delete-agent" => {
            let agent_id = params
                .get("agentId")
                .and_then(Value::as_str)
                .ok_or("agentId parameter required")?;

            if let Some(list) = config
                .pointer_mut("/agents/list")
                .and_then(Value::as_array_mut)
            {
                list.retain(|a| a.get("id").and_then(Value::as_str) != Some(agent_id));
            }

            // Reset bindings that reference this agent to "main"
            if let Some(bindings) = config
                .pointer_mut("/bindings")
                .and_then(Value::as_array_mut)
            {
                for b in bindings.iter_mut() {
                    if b.get("agentId").and_then(Value::as_str) == Some(agent_id) {
                        if let Some(obj) = b.as_object_mut() {
                            obj.insert("agentId".into(), json!("main"));
                        }
                    }
                }
            }
        }
        "set-channel-model" => {
            let channel_path = params
                .get("channelPath")
                .and_then(Value::as_str)
                .ok_or("channelPath parameter required")?;
            let model = params.get("model").and_then(Value::as_str);
            let path = format!("{channel_path}.model");
            match model {
                Some(m) => {
                    crate::doctor::upsert_json_path(config, &path, json!(m))?;
                }
                None => {
                    crate::doctor::delete_json_path(config, &path);
                }
            }
        }
        "update-channel-config" => {
            let channel_path = params
                .get("channelPath")
                .and_then(Value::as_str)
                .ok_or("channelPath parameter required")?;

            if let Some(channel_type) = params.get("type").and_then(Value::as_str) {
                let path = format!("{channel_path}.type");
                crate::doctor::upsert_json_path(config, &path, json!(channel_type))?;
            }
            if let Some(mode) = params.get("mode").and_then(Value::as_str) {
                let path = format!("{channel_path}.mode");
                crate::doctor::upsert_json_path(config, &path, json!(mode))?;
            }
            if let Some(allowlist) = params.get("allowlist").and_then(Value::as_array) {
                let path = format!("{channel_path}.allowlist");
                crate::doctor::upsert_json_path(config, &path, json!(allowlist.clone()))?;
            }
            if let Some(model) = params.get("model").and_then(Value::as_str) {
                let path = format!("{channel_path}.model");
                crate::doctor::upsert_json_path(config, &path, json!(model))?;
            }
        }
        "set-binding-agent" => {
            let binding_index = params
                .get("index")
                .and_then(Value::as_u64)
                .ok_or("index parameter required")? as usize;
            let agent_id = params
                .get("agentId")
                .and_then(Value::as_str)
                .ok_or("agentId parameter required")?;

            if let Some(bindings) = config
                .pointer_mut("/bindings")
                .and_then(Value::as_array_mut)
            {
                if binding_index < bindings.len() {
                    if let Some(obj) = bindings[binding_index].as_object_mut() {
                        obj.insert("agentId".into(), json!(agent_id));
                    }
                }
            }
        }
        "add-binding" => {
            let channel = params
                .get("channel")
                .and_then(Value::as_str)
                .ok_or("channel parameter required")?;
            let agent_id = params
                .get("agentId")
                .and_then(Value::as_str)
                .ok_or("agentId parameter required")?;
            let pattern = params.get("pattern").and_then(Value::as_str);

            let mut binding = serde_json::json!({
                "channel": channel,
                "agentId": agent_id
            });
            if let Some(p) = pattern {
                binding["pattern"] = json!(p);
            }

            if let Some(bindings) = config
                .pointer_mut("/bindings")
                .and_then(Value::as_array_mut)
            {
                bindings.push(binding);
            } else {
                crate::doctor::upsert_json_path(config, "bindings", json!([binding]))?;
            }
        }
        _ => return Err(format!("unknown patch template: {template}")),
    }
    Ok(())
}

/// Collect paths that differ between two config values
fn collect_change_paths(before: &Value, after: &Value) -> Vec<String> {
    let mut paths = Vec::new();
    collect_diff_paths("", before, after, &mut paths);
    paths.sort();
    paths.dedup();
    paths
}

fn collect_diff_paths(prefix: &str, before: &Value, after: &Value, out: &mut Vec<String>) {
    match (before, after) {
        (Value::Object(before_obj), Value::Object(after_obj)) => {
            let all_keys: std::collections::HashSet<&String> =
                before_obj.keys().chain(after_obj.keys()).collect();
            for key in all_keys {
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                match (before_obj.get(key), after_obj.get(key)) {
                    (Some(b), Some(a)) if b != a => {
                        collect_diff_paths(&new_prefix, b, a, out);
                    }
                    (None, Some(_)) | (Some(_), None) => {
                        out.push(new_prefix);
                    }
                    _ => {}
                }
            }
        }
        (b, a) if b != a => {
            out.push(prefix.to_string());
        }
        _ => {}
    }
}

/// Get a value from config by JSON path
pub fn get_config_value<'a>(config: &'a Value, path: &str) -> Option<&'a Value> {
    crate::doctor::json_path_get(config, path)
}

/// Set a value in config by JSON path
pub fn set_config_value(config: &mut Value, path: &str, value: Value) -> Result<(), String> {
    crate::doctor::upsert_json_path(config, path, value)
}

/// Delete a value from config by JSON path
pub fn delete_config_value(config: &mut Value, path: &str) -> bool {
    crate::doctor::delete_json_path(config, path)
}

/// Validate that content is valid config JSON
pub fn validate_config_json(content: &str) -> Result<Value, String> {
    crate::doctor::parse_json_document(content, "config")
}

/// Format a diff between two configs for display
pub fn format_config_diff(before: &Value, after: &Value) -> String {
    let changes = collect_change_paths(before, after);
    if changes.is_empty() {
        return "No changes".to_string();
    }
    changes.join("\n")
}

/// Extract model bindings from config
pub fn extract_model_bindings(config: &Value) -> Vec<ModelBinding> {
    let mut bindings = Vec::new();

    // Global default
    let global = config
        .pointer("/agents/defaults/model")
        .or_else(|| config.pointer("/agents/default/model"))
        .and_then(read_model_value);
    bindings.push(ModelBinding {
        scope: "global".into(),
        scope_id: "global".into(),
        model_value: global,
        path: Some("agents.defaults.model".into()),
    });

    // Agent-specific
    if let Some(agents) = config.pointer("/agents/list").and_then(Value::as_array) {
        for agent in agents {
            let id = agent.get("id").and_then(Value::as_str).unwrap_or("agent");
            let model = agent.get("model").and_then(read_model_value);
            bindings.push(ModelBinding {
                scope: "agent".into(),
                scope_id: id.to_string(),
                model_value: model,
                path: Some(format!("agents.list.{id}.model")),
            });
        }
    }

    // Channel-specific
    fn walk_channel_bindings(prefix: &str, node: &Value, out: &mut Vec<ModelBinding>) {
        if let Some(obj) = node.as_object() {
            if let Some(model) = obj.get("model").and_then(read_model_value) {
                out.push(ModelBinding {
                    scope: "channel".into(),
                    scope_id: prefix.to_string(),
                    model_value: Some(model),
                    path: Some(format!("{prefix}.model")),
                });
            }
            for (k, child) in obj {
                if let Value::Object(_) = child {
                    walk_channel_bindings(&format!("{prefix}.{k}"), child, out);
                }
            }
        }
    }

    if let Some(channels) = config.get("channels") {
        walk_channel_bindings("channels", channels, &mut bindings);
    }

    bindings
}

/// Model binding information
#[derive(Debug, Clone)]
pub struct ModelBinding {
    pub scope: String,
    pub scope_id: String,
    pub model_value: Option<String>,
    pub path: Option<String>,
}

fn read_model_value(value: &Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }
    if let Some(obj) = value.as_object() {
        if let Some(primary) = obj.get("primary").and_then(Value::as_str) {
            return Some(primary.to_string());
        }
        if let Some(name) = obj.get("name").and_then(Value::as_str) {
            return Some(name.to_string());
        }
        if let Some(model) = obj.get("model").and_then(Value::as_str) {
            return Some(model.to_string());
        }
        if let Some(default) = obj.get("default").and_then(Value::as_str) {
            return Some(default.to_string());
        }
        if let Some(provider) = obj.get("provider").and_then(Value::as_str) {
            if let Some(id) = obj.get("id").and_then(Value::as_str) {
                return Some(format!("{provider}/{id}"));
            }
        }
    }
    None
}

/// Collect agent IDs from config
pub fn collect_agent_ids(config: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(agents) = config.pointer("/agents/list").and_then(Value::as_array) {
        for agent in agents {
            if let Some(id) = agent.get("id").and_then(Value::as_str) {
                ids.push(id.to_string());
            }
        }
    }
    if ids.is_empty() {
        ids.push("main".into());
    }
    ids
}

/// Check if an agent exists in config
pub fn agent_exists(config: &Value, agent_id: &str) -> bool {
    collect_agent_ids(config).iter().any(|id| id == agent_id)
}

/// Get channel node information from config
pub fn collect_channel_nodes(config: &Value) -> Vec<ChannelNode> {
    let mut out = Vec::new();
    if let Some(channels) = config.get("channels") {
        walk_channel_nodes("channels", channels, &mut out);
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk_channel_nodes(prefix: &str, node: &Value, out: &mut Vec<ChannelNode>) {
    let Some(obj) = node.as_object() else {
        return;
    };

    if is_channel_like_node(prefix, obj) {
        let channel_type = resolve_channel_type(prefix, obj);
        let mode = resolve_channel_mode(obj);
        let allowlist = collect_channel_allowlist(obj);
        let has_model_field = obj.contains_key("model");
        let model = obj.get("model").and_then(read_model_value);
        out.push(ChannelNode {
            path: prefix.to_string(),
            channel_type,
            mode,
            allowlist,
            model,
            has_model_field,
        });
    }

    for (key, child) in obj {
        if key == "allowlist" || key == "model" || key == "mode" {
            continue;
        }
        if let Value::Object(_) = child {
            walk_channel_nodes(&format!("{prefix}.{key}"), child, out);
        }
    }
}

fn is_channel_like_node(prefix: &str, obj: &serde_json::Map<String, Value>) -> bool {
    if prefix == "channels" {
        return false;
    }
    if obj.contains_key("model")
        || obj.contains_key("type")
        || obj.contains_key("mode")
        || obj.contains_key("policy")
        || obj.contains_key("allowlist")
        || obj.contains_key("allowFrom")
        || obj.contains_key("groupAllowFrom")
        || obj.contains_key("dmPolicy")
        || obj.contains_key("groupPolicy")
        || obj.contains_key("guilds")
        || obj.contains_key("accounts")
        || obj.contains_key("dm")
        || obj.contains_key("users")
        || obj.contains_key("enabled")
        || obj.contains_key("token")
        || obj.contains_key("botToken")
    {
        return true;
    }
    if prefix.contains(".accounts.") || prefix.contains(".guilds.") || prefix.contains(".channels.")
    {
        return true;
    }
    if prefix.ends_with(".dm") || prefix.ends_with(".default") {
        return true;
    }
    false
}

fn resolve_channel_type(prefix: &str, obj: &serde_json::Map<String, Value>) -> Option<String> {
    obj.get("type")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            if prefix.ends_with(".dm") {
                Some("dm".into())
            } else if prefix.contains(".accounts.") {
                Some("account".into())
            } else if prefix.contains(".channels.") && prefix.contains(".guilds.") {
                Some("channel".into())
            } else if prefix.contains(".guilds.") {
                Some("guild".into())
            } else if obj.contains_key("guilds") || obj.contains_key("accounts") {
                Some("platform".into())
            } else {
                None
            }
        })
}

fn resolve_channel_mode(obj: &serde_json::Map<String, Value>) -> Option<String> {
    let mut modes: Vec<String> = Vec::new();
    if let Some(v) = obj.get("mode").and_then(Value::as_str) {
        modes.push(v.to_string());
    }
    if let Some(v) = obj.get("policy").and_then(Value::as_str) {
        if !modes.iter().any(|m| m == v) {
            modes.push(v.to_string());
        }
    }
    if let Some(v) = obj.get("dmPolicy").and_then(Value::as_str) {
        if !modes.iter().any(|m| m == v) {
            modes.push(v.to_string());
        }
    }
    if let Some(v) = obj.get("groupPolicy").and_then(Value::as_str) {
        if !modes.iter().any(|m| m == v) {
            modes.push(v.to_string());
        }
    }
    if modes.is_empty() {
        None
    } else {
        Some(modes.join(" / "))
    }
}

fn collect_channel_allowlist(obj: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut uniq = std::collections::HashSet::<String>::new();
    for key in ["allowlist", "allowFrom", "groupAllowFrom"] {
        if let Some(values) = obj.get(key).and_then(Value::as_array) {
            for value in values.iter().filter_map(Value::as_str) {
                let next = value.to_string();
                if uniq.insert(next.clone()) {
                    out.push(next);
                }
            }
        }
    }
    if let Some(values) = obj.get("users").and_then(Value::as_array) {
        for value in values.iter().filter_map(Value::as_str) {
            let next = value.to_string();
            if uniq.insert(next.clone()) {
                out.push(next);
            }
        }
    }
    out
}

/// Channel node information
#[derive(Debug, Clone)]
pub struct ChannelNode {
    pub path: String,
    pub channel_type: Option<String>,
    pub mode: Option<String>,
    pub allowlist: Vec<String>,
    pub model: Option<String>,
    pub has_model_field: bool,
}

/// Resolve gateway port from config
pub fn resolve_gateway_port(config: &Value) -> u16 {
    crate::doctor::resolve_gateway_port_from_config(config)
}

/// Resolve agent workspace from config
pub fn resolve_agent_workspace(
    config: &Value,
    agent_id: &str,
    fallback_default: Option<&str>,
) -> Result<String, String> {
    crate::doctor::resolve_agent_workspace_from_config(config, agent_id, fallback_default)
}

/// Generate snapshot filename from timestamp and source
pub fn snapshot_filename(ts: u64, source: &str) -> String {
    format!("{ts}-{source}.json")
}

/// Parse snapshot filename to extract (timestamp, source)
pub fn parse_snapshot_filename(filename: &str) -> Option<(u64, String)> {
    let stem = filename.trim_end_matches(".json");
    let parts: Vec<&str> = stem.splitn(3, '-').collect();
    if parts.len() < 2 {
        return None;
    }
    let ts = parts[0].parse::<u64>().ok()?;
    let source = parts.get(1).map(|s| s.to_string()).unwrap_or_default();
    Some((ts, source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_and_normalize_config_valid_json() {
        let raw = r#"{"gateway":{"port":18789}}"#;
        let (parsed, normalized) = parse_and_normalize_config(raw).expect("parse");
        assert_eq!(parsed["gateway"]["port"], 18789);
        assert!(normalized.contains('\n')); // Pretty printed
    }

    #[test]
    fn parse_config_json5_accepts_trailing_comma() {
        let raw = r#"{gateway:{port:18789,},}"#;
        let parsed = parse_config_json5(raw);
        assert_eq!(parsed["gateway"]["port"], 18789);
    }

    #[test]
    fn prepare_config_write_returns_both_texts() {
        let current = r#"{"gateway":{"port":18789}}"#;
        let next = json!({"gateway":{"port":19789}});
        let (new_text, snapshot_text) =
            prepare_config_write(current, &next, "test").expect("prepare");
        assert!(new_text.contains("19789"));
        assert!(snapshot_text.contains("18789"));
    }

    #[test]
    fn build_candidate_config_applies_template() {
        let current = json!({"gateway":{"port":18789}});
        let mut params = serde_json::Map::new();
        params.insert("port".into(), json!(19789_u64));
        let (candidate, changes) =
            build_candidate_config(&current, "set-gateway-port", &params).expect("build");
        assert_eq!(candidate["gateway"]["port"], 19789);
        assert!(changes.iter().any(|c| c.contains("port")));
    }

    #[test]
    fn get_set_delete_config_value() {
        let mut config = json!({"a":{"b":1}});

        assert_eq!(
            get_config_value(&config, "a.b").and_then(Value::as_i64),
            Some(1)
        );

        set_config_value(&mut config, "a.c", json!(2)).expect("set");
        assert_eq!(config["a"]["c"], 2);

        assert!(delete_config_value(&mut config, "a.b"));
        assert!(get_config_value(&config, "a.b").is_none());
    }

    #[test]
    fn collect_change_paths_detects_differences() {
        let before = json!({"a":1,"b":{"c":2}});
        let after = json!({"a":1,"b":{"c":3,"d":4}});
        let changes = collect_change_paths(&before, &after);
        assert!(changes.iter().any(|p| p.contains("c")));
        assert!(changes.iter().any(|p| p.contains("d")));
    }

    #[test]
    fn extract_model_bindings_finds_all_scopes() {
        let config = json!({
            "agents": {
                "defaults": {"model": "global/model"},
                "list": [{"id": "agent1", "model": "agent/model"}]
            },
            "channels": {
                "discord": {"model": "channel/model"}
            }
        });
        let bindings = extract_model_bindings(&config);
        assert_eq!(bindings.len(), 3);
        assert!(bindings.iter().any(|b| b.scope == "global"));
        assert!(bindings.iter().any(|b| b.scope == "agent"));
        assert!(bindings.iter().any(|b| b.scope == "channel"));
    }

    #[test]
    fn collect_agent_ids_includes_main_fallback() {
        let config = json!({"agents": {"list": [{"id": "agent1"}]}});
        let ids = collect_agent_ids(&config);
        assert!(ids.contains(&"agent1".to_string()));

        let empty_config = json!({});
        let empty_ids = collect_agent_ids(&empty_config);
        assert_eq!(empty_ids, vec!["main"]);
    }

    #[test]
    fn collect_channel_nodes_finds_channel_configs() {
        let config = json!({
            "channels": {
                "discord": {
                    "type": "discord",
                    "model": "test/model",
                    "allowlist": ["user1"]
                }
            }
        });
        let nodes = collect_channel_nodes(&config);
        assert!(!nodes.is_empty());
        let discord = nodes
            .iter()
            .find(|n| n.path == "channels.discord")
            .expect("discord node");
        assert_eq!(discord.channel_type, Some("discord".into()));
        assert!(discord.has_model_field);
    }

    #[test]
    fn resolve_gateway_port_default() {
        let config = json!({});
        assert_eq!(resolve_gateway_port(&config), 18789);

        let config_with_port = json!({"gateway":{"port":19789}});
        assert_eq!(resolve_gateway_port(&config_with_port), 19789);
    }

    #[test]
    fn snapshot_filename_format() {
        assert_eq!(
            snapshot_filename(1234567890, "test"),
            "1234567890-test.json"
        );
    }

    #[test]
    fn parse_snapshot_filename_extracts_parts() {
        let (ts, source) = parse_snapshot_filename("1234567890-test-snapshot.json").expect("parse");
        assert_eq!(ts, 1234567890);
        assert_eq!(source, "test");
    }

    #[test]
    fn agent_exists_check() {
        let config = json!({"agents": {"list": [{"id": "test-agent"}]}});
        assert!(agent_exists(&config, "test-agent"));
        assert!(!agent_exists(&config, "nonexistent"));
    }

    #[test]
    fn format_config_diff_shows_changes() {
        let before = json!({"a":1});
        let after = json!({"a":2});
        let diff = format_config_diff(&before, &after);
        assert!(!diff.is_empty());
        assert_ne!(diff, "No changes");
    }

    #[test]
    fn validate_config_json_rejects_invalid() {
        assert!(validate_config_json("{invalid}").is_err());
        assert!(validate_config_json(r#"{"valid":true}"#).is_ok());
    }

    #[test]
    fn build_candidate_config_create_agent() {
        let current = json!({"agents": {"list": []}});
        let mut params = serde_json::Map::new();
        params.insert("agentId".into(), json!("new-agent"));
        params.insert("model".into(), json!("test/model"));
        params.insert("independent".into(), json!(true));

        let (candidate, _) =
            build_candidate_config(&current, "create-agent", &params).expect("build");

        let agents = candidate["agents"]["list"].as_array().expect("list");
        assert!(agents.iter().any(|a| a["id"] == "new-agent"));
    }

    #[test]
    fn build_candidate_config_delete_agent() {
        let current = json!({
            "agents": {
                "list": [{"id": "to-delete"}, {"id": "keep"}]
            },
            "bindings": [{"agentId": "to-delete", "channel": "test"}]
        });
        let mut params = serde_json::Map::new();
        params.insert("agentId".into(), json!("to-delete"));

        let (candidate, _) =
            build_candidate_config(&current, "delete-agent", &params).expect("build");

        let agents = candidate["agents"]["list"].as_array().expect("list");
        assert!(!agents.iter().any(|a| a["id"] == "to-delete"));
        assert!(agents.iter().any(|a| a["id"] == "keep"));

        // Binding should be reset to main
        let bindings = candidate["bindings"].as_array().expect("bindings");
        assert!(bindings.iter().any(|b| b["agentId"] == "main"));
    }
}
