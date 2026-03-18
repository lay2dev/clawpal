use super::*;

pub(crate) fn collect_channel_summary(cfg: &Value) -> ChannelSummary {
    let examples = collect_channel_model_overrides_list(cfg);
    let configured_channels = cfg
        .get("channels")
        .and_then(|v| v.as_object())
        .map(|channels| channels.len())
        .unwrap_or(0);

    ChannelSummary {
        configured_channels,
        channel_model_overrides: examples.len(),
        channel_examples: examples,
    }
}

pub(crate) fn collect_channel_model_overrides(cfg: &Value) -> Vec<String> {
    collect_channel_model_overrides_list(cfg)
}

pub(crate) fn collect_channel_model_overrides_list(cfg: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(channels) = cfg.get("channels").and_then(Value::as_object) {
        for (name, entry) in channels {
            let mut branch = Vec::new();
            collect_channel_paths(name, entry, &mut branch);
            out.extend(branch);
        }
    }
    out
}

pub(crate) fn collect_channel_paths(prefix: &str, node: &Value, out: &mut Vec<String>) {
    if let Some(obj) = node.as_object() {
        if let Some(model) = obj.get("model").and_then(read_model_value) {
            out.push(format!("{prefix} => {model}"));
        }
        for (key, child) in obj {
            if key == "model" {
                continue;
            }
            let next = format!("{prefix}.{key}");
            collect_channel_paths(&next, child, out);
        }
    }
}

pub(crate) fn collect_channel_nodes(cfg: &Value) -> Vec<ChannelNode> {
    let mut out = Vec::new();
    if let Some(channels) = cfg.get("channels") {
        walk_channel_nodes("channels", channels, &mut out);
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

pub(crate) fn walk_channel_nodes(prefix: &str, node: &Value, out: &mut Vec<ChannelNode>) {
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
            display_name: None,
            name_status: None,
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

pub(crate) fn enrich_channel_display_names(
    paths: &crate::models::OpenClawPaths,
    cfg: &Value,
    nodes: &mut [ChannelNode],
) -> Result<(), String> {
    let mut grouped: BTreeMap<String, Vec<(usize, String, String)>> = BTreeMap::new();
    let mut local_names: Vec<(usize, String)> = Vec::new();

    for (index, node) in nodes.iter().enumerate() {
        if let Some((plugin, identifier, kind)) = resolve_channel_node_identity(cfg, node) {
            grouped
                .entry(plugin)
                .or_default()
                .push((index, identifier, kind));
        }
        if node.display_name.is_none() {
            if let Some(local_name) = channel_node_local_name(cfg, &node.path) {
                local_names.push((index, local_name));
            }
        }
    }
    for (index, local_name) in local_names {
        if let Some(node) = nodes.get_mut(index) {
            node.display_name = Some(local_name);
            node.name_status = Some("local".into());
        }
    }

    let cache_file = paths.clawpal_dir.join("channel-name-cache.json");
    if nodes.is_empty() {
        if cache_file.exists() {
            let _ = fs::remove_file(&cache_file);
        }
        return Ok(());
    }

    for (plugin, entries) in grouped {
        if entries.is_empty() {
            continue;
        }
        let ids: Vec<String> = entries
            .iter()
            .map(|(_, identifier, _)| identifier.clone())
            .collect();
        let kind = &entries[0].2;
        let mut args = vec![
            "channels".to_string(),
            "resolve".to_string(),
            "--json".to_string(),
            "--channel".to_string(),
            plugin.clone(),
            "--kind".to_string(),
            kind.clone(),
        ];
        for entry in &ids {
            args.push(entry.clone());
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = match run_openclaw_raw(&args) {
            Ok(output) => output,
            Err(_) => {
                for (index, _, _) in entries {
                    nodes[index].name_status = Some("resolve failed".into());
                }
                continue;
            }
        };
        if output.stdout.trim().is_empty() {
            for (index, _, _) in entries {
                nodes[index].name_status = Some("unresolved".into());
            }
            continue;
        }
        let json_str =
            clawpal_core::doctor::extract_json_from_output(&output.stdout).unwrap_or("[]");
        let parsed: Vec<Value> = serde_json::from_str(json_str).unwrap_or_default();
        let mut name_map = HashMap::new();
        for item in parsed {
            let input = item
                .get("input")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let resolved = item
                .get("resolved")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let note = item
                .get("note")
                .and_then(Value::as_str)
                .map(|value| value.to_string());
            if !input.is_empty() {
                name_map.insert(input, (resolved, name, note));
            }
        }

        for (index, identifier, _) in entries {
            if let Some((resolved, name, note)) = name_map.get(&identifier) {
                if *resolved {
                    if let Some(name) = name {
                        nodes[index].display_name = Some(name.clone());
                        nodes[index].name_status = Some("resolved".into());
                    } else {
                        nodes[index].name_status = Some("resolved".into());
                    }
                } else if let Some(note) = note {
                    nodes[index].name_status = Some(note.clone());
                } else {
                    nodes[index].name_status = Some("unresolved".into());
                }
            } else {
                nodes[index].name_status = Some("unresolved".into());
            }
        }
    }

    let _ = save_json_cache(&cache_file, nodes);
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ChannelNameCacheEntry {
    pub path: String,
    pub display_name: Option<String>,
    pub name_status: Option<String>,
}

pub(crate) fn save_json_cache(cache_file: &Path, nodes: &[ChannelNode]) -> Result<(), String> {
    let payload: Vec<ChannelNameCacheEntry> = nodes
        .iter()
        .map(|node| ChannelNameCacheEntry {
            path: node.path.clone(),
            display_name: node.display_name.clone(),
            name_status: node.name_status.clone(),
        })
        .collect();
    write_text(
        cache_file,
        &serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?,
    )
}

pub(crate) fn resolve_channel_node_identity(
    cfg: &Value,
    node: &ChannelNode,
) -> Option<(String, String, String)> {
    let parts: Vec<&str> = node.path.split('.').collect();
    if parts.len() < 2 || parts[0] != "channels" {
        return None;
    }
    let plugin = parts[1].to_string();
    let identifier = channel_last_segment(node.path.as_str())?;
    let config_node = channel_lookup_node(cfg, &node.path);
    let kind = if node.channel_type.as_deref() == Some("dm") || node.path.ends_with(".dm") {
        "user".to_string()
    } else if config_node
        .and_then(|value| {
            value
                .get("users")
                .or(value.get("members"))
                .or_else(|| value.get("peerIds"))
        })
        .is_some()
    {
        "user".to_string()
    } else {
        "group".to_string()
    };
    Some((plugin, identifier, kind))
}

pub(crate) fn channel_last_segment(path: &str) -> Option<String> {
    path.split('.').next_back().map(|value| value.to_string())
}

pub(crate) fn channel_node_local_name(cfg: &Value, path: &str) -> Option<String> {
    channel_lookup_node(cfg, path).and_then(|node| {
        if let Some(slug) = node.get("slug").and_then(Value::as_str) {
            let trimmed = slug.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        if let Some(name) = node.get("name").and_then(Value::as_str) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        None
    })
}

pub(crate) fn channel_lookup_node<'a>(cfg: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = cfg;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

pub(crate) fn is_channel_like_node(prefix: &str, obj: &serde_json::Map<String, Value>) -> bool {
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

pub(crate) fn resolve_channel_type(
    prefix: &str,
    obj: &serde_json::Map<String, Value>,
) -> Option<String> {
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
            } else if obj.contains_key("guilds") {
                Some("platform".into())
            } else if obj.contains_key("accounts") {
                Some("platform".into())
            } else {
                None
            }
        })
}

pub(crate) fn resolve_channel_mode(obj: &serde_json::Map<String, Value>) -> Option<String> {
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

pub(crate) fn collect_channel_allowlist(obj: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut uniq = HashSet::<String>::new();
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
