use super::*;

#[tauri::command]
pub async fn remote_setup_agent_identity(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    agent_id: String,
    name: String,
    emoji: Option<String>,
) -> Result<bool, String> {
    let agent_id = agent_id.trim().to_string();
    let name = name.trim().to_string();
    if agent_id.is_empty() {
        return Err("Agent ID is required".into());
    }
    if name.is_empty() {
        return Err("Name is required".into());
    }

    // Read remote config to find agent workspace
    let (_config_path, _raw, cfg) = remote_read_openclaw_config_text_and_json(&pool, &host_id)
        .await
        .map_err(|e| format!("Failed to parse config: {e}"))?;

    let workspace = clawpal_core::doctor::resolve_agent_workspace_from_config(
        &cfg,
        &agent_id,
        Some("~/.openclaw/agents"),
    )?;

    // Build IDENTITY.md content
    let mut content = format!("- Name: {}\n", name);
    if let Some(ref e) = emoji {
        let e = e.trim();
        if !e.is_empty() {
            content.push_str(&format!("- Emoji: {}\n", e));
        }
    }

    // Write via SSH
    let ws = if workspace.starts_with("~/") {
        workspace.to_string()
    } else {
        format!("~/{workspace}")
    };
    pool.exec(&host_id, &format!("mkdir -p {}", shell_escape(&ws)))
        .await?;
    let identity_path = format!("{}/IDENTITY.md", ws);
    pool.sftp_write(&host_id, &identity_path, &content).await?;

    Ok(true)
}

#[tauri::command]
pub async fn remote_chat_via_openclaw(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    agent_id: String,
    message: String,
    session_id: Option<String>,
) -> Result<Value, String> {
    let escaped_msg = message.replace('\'', "'\\''");
    let escaped_agent = agent_id.replace('\'', "'\\''");
    let mut cmd = format!(
        "openclaw agent --local --agent '{}' --message '{}' --json --no-color",
        escaped_agent, escaped_msg
    );
    if let Some(sid) = session_id {
        let escaped_sid = sid.replace('\'', "'\\''");
        cmd.push_str(&format!(" --session-id '{}'", escaped_sid));
    }
    let result = pool.exec_login(&host_id, &cmd).await?;
    // Try to extract JSON from stdout first — even on non-zero exit the
    // command may have produced valid output (e.g. bash job-control warnings
    // in stderr cause exit 1 but the actual command succeeded).
    if let Some(json_str) = clawpal_core::doctor::extract_json_from_output(&result.stdout) {
        return serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse remote chat response: {e}"));
    }
    if result.exit_code != 0 {
        return Err(format!(
            "Remote chat failed (exit {}): {}",
            result.exit_code, result.stderr
        ));
    }
    Err(format!(
        "No JSON in remote openclaw output: {}",
        result.stdout
    ))
}

#[tauri::command]
pub fn create_agent(
    agent_id: String,
    model_value: Option<String>,
    independent: Option<bool>,
) -> Result<AgentOverview, String> {
    let agent_id = agent_id.trim().to_string();
    if agent_id.is_empty() {
        return Err("Agent ID is required".into());
    }
    if !agent_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Agent ID may only contain letters, numbers, hyphens, and underscores".into());
    }

    let paths = resolve_paths();
    let mut cfg = read_openclaw_config(&paths)?;
    let current = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;

    let existing_ids = collect_agent_ids(&cfg);
    if existing_ids
        .iter()
        .any(|id| id.eq_ignore_ascii_case(&agent_id))
    {
        return Err(format!("Agent '{}' already exists", agent_id));
    }

    let model_display = model_value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    // If independent, create a dedicated workspace directory;
    // otherwise inherit the default workspace so the gateway doesn't auto-create one.
    let workspace = if independent.unwrap_or(false) {
        let ws_dir = paths.base_dir.join("workspaces").join(&agent_id);
        fs::create_dir_all(&ws_dir).map_err(|e| e.to_string())?;
        let ws_path = ws_dir.to_string_lossy().to_string();
        Some(ws_path)
    } else {
        cfg.pointer("/agents/defaults/workspace")
            .or_else(|| cfg.pointer("/agents/default/workspace"))
            .and_then(Value::as_str)
            .map(|s| s.to_string())
    };

    // Build agent entry
    let mut agent_obj = serde_json::Map::new();
    agent_obj.insert("id".into(), Value::String(agent_id.clone()));
    if let Some(ref model_str) = model_display {
        agent_obj.insert("model".into(), Value::String(model_str.clone()));
    }
    if let Some(ref ws) = workspace {
        agent_obj.insert("workspace".into(), Value::String(ws.clone()));
    }

    let agents = cfg
        .as_object_mut()
        .ok_or("config is not an object")?
        .entry("agents")
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .ok_or("agents is not an object")?;
    let list = agents
        .entry("list")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or("agents.list is not an array")?;
    list.push(Value::Object(agent_obj));

    write_config_with_snapshot(&paths, &current, &cfg, "create-agent")?;
    Ok(AgentOverview {
        id: agent_id,
        name: None,
        emoji: None,
        model: model_display,
        channels: vec![],
        online: false,
        workspace,
    })
}

#[tauri::command]
pub fn delete_agent(agent_id: String) -> Result<bool, String> {
    let agent_id = agent_id.trim().to_string();
    if agent_id.is_empty() {
        return Err("Agent ID is required".into());
    }
    if agent_id == "main" {
        return Err("Cannot delete the main agent".into());
    }

    let paths = resolve_paths();
    let mut cfg = read_openclaw_config(&paths)?;
    let current = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;

    let list = cfg
        .pointer_mut("/agents/list")
        .and_then(Value::as_array_mut)
        .ok_or("agents.list not found")?;

    let before = list.len();
    list.retain(|agent| agent.get("id").and_then(Value::as_str) != Some(&agent_id));

    if list.len() == before {
        return Err(format!("Agent '{}' not found", agent_id));
    }

    // Reset any bindings that reference this agent back to "main" (default)
    // so the channel doesn't lose its binding entry entirely.
    if let Some(bindings) = cfg.pointer_mut("/bindings").and_then(Value::as_array_mut) {
        for b in bindings.iter_mut() {
            if b.get("agentId").and_then(Value::as_str) == Some(&agent_id) {
                if let Some(obj) = b.as_object_mut() {
                    obj.insert("agentId".into(), Value::String("main".into()));
                }
            }
        }
    }

    write_config_with_snapshot(&paths, &current, &cfg, "delete-agent")?;
    Ok(true)
}

#[tauri::command]
pub fn setup_agent_identity(
    agent_id: String,
    name: String,
    emoji: Option<String>,
) -> Result<bool, String> {
    let agent_id = agent_id.trim().to_string();
    let name = name.trim().to_string();
    if agent_id.is_empty() {
        return Err("Agent ID is required".into());
    }
    if name.is_empty() {
        return Err("Name is required".into());
    }

    let paths = resolve_paths();
    let cfg = read_openclaw_config(&paths)?;

    let workspace =
        clawpal_core::doctor::resolve_agent_workspace_from_config(&cfg, &agent_id, None)
            .map(|s| expand_tilde(&s))?;

    // Build IDENTITY.md content
    let mut content = format!("- Name: {}\n", name);
    if let Some(ref e) = emoji {
        let e = e.trim();
        if !e.is_empty() {
            content.push_str(&format!("- Emoji: {}\n", e));
        }
    }

    let ws_path = std::path::Path::new(&workspace);
    fs::create_dir_all(ws_path).map_err(|e| format!("Failed to create workspace dir: {}", e))?;
    let identity_path = ws_path.join("IDENTITY.md");
    fs::write(&identity_path, &content)
        .map_err(|e| format!("Failed to write IDENTITY.md: {}", e))?;

    Ok(true)
}

#[tauri::command]
pub async fn chat_via_openclaw(
    agent_id: String,
    message: String,
    session_id: Option<String>,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = resolve_paths();
        if let Err(err) = sync_main_auth_for_active_config(&paths) {
            eprintln!("Warning: pre-chat main auth sync failed: {err}");
        }
        let mut args = vec![
            "agent".to_string(),
            "--local".to_string(),
            "--agent".to_string(),
            agent_id,
            "--message".to_string(),
            message,
            "--json".to_string(),
            "--no-color".to_string(),
        ];
        if let Some(sid) = session_id {
            args.push("--session-id".to_string());
            args.push(sid);
        }

        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = run_openclaw_raw(&arg_refs)?;
        let json_str = clawpal_core::doctor::extract_json_from_output(&output.stdout)
            .ok_or_else(|| format!("No JSON in openclaw output: {}", output.stdout))?;
        serde_json::from_str(json_str).map_err(|e| format!("Parse openclaw response failed: {}", e))
    })
    .await
    .map_err(|e| format!("Task join failed: {}", e))?
}
