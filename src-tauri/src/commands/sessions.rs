use super::*;

#[tauri::command]
pub async fn remote_analyze_sessions(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<AgentSessionAnalysis>, String> {
    timed_async!("remote_analyze_sessions", {
        // Run a shell script via SSH that scans session files and outputs JSON.
        // This is MUCH faster than doing per-file SFTP reads.
        let script = r#"
    setopt nonomatch 2>/dev/null; shopt -s nullglob 2>/dev/null
    cd ~/.openclaw/agents 2>/dev/null || { echo '[]'; exit 0; }
    now=$(date +%s)
    sep=""
    echo "["
    for agent_dir in */; do
      [ -d "$agent_dir" ] || continue
      agent="${agent_dir%/}"
      # Sanitize agent name for JSON (escape backslash then double-quote)
      safe_agent=$(printf '%s' "$agent" | sed 's/\\/\\\\/g; s/"/\\"/g')
      for kind in sessions sessions_archive; do
        dir="$agent_dir$kind"
        [ -d "$dir" ] || continue
        for f in "$dir"/*.jsonl; do
          [ -f "$f" ] || continue
          fname=$(basename "$f" .jsonl)
          safe_fname=$(printf '%s' "$fname" | sed 's/\\/\\\\/g; s/"/\\"/g')
          size=$(wc -c < "$f" 2>/dev/null | tr -d ' ')
          msgs=$(grep -c '"type":"message"' "$f" 2>/dev/null || true)
          [ -z "$msgs" ] && msgs=0
          user_msgs=$(grep -c '"role":"user"' "$f" 2>/dev/null || true)
          [ -z "$user_msgs" ] && user_msgs=0
          asst_msgs=$(grep -c '"role":"assistant"' "$f" 2>/dev/null || true)
          [ -z "$asst_msgs" ] && asst_msgs=0
          mtime=$(stat -c %Y "$f" 2>/dev/null || stat -f %m "$f" 2>/dev/null || echo 0)
          age_days=$(( (now - mtime) / 86400 ))
          printf '%s{"agent":"%s","sessionId":"%s","sizeBytes":%s,"messageCount":%s,"userMessageCount":%s,"assistantMessageCount":%s,"ageDays":%s,"kind":"%s"}' \
            "$sep" "$safe_agent" "$safe_fname" "$size" "$msgs" "$user_msgs" "$asst_msgs" "$age_days" "$kind"
          sep=","
        done
      done
    done
    echo "]"
    "#;

        let result = pool.exec(&host_id, script).await?;
        if result.exit_code != 0 && result.stdout.trim().is_empty() {
            // No agents directory — return empty
            return Ok(Vec::new());
        }

        let core = clawpal_core::sessions::parse_session_analysis(result.stdout.trim())?;
        Ok(core
            .into_iter()
            .map(|agent| AgentSessionAnalysis {
                agent: agent.agent,
                total_files: agent.total_files,
                total_size_bytes: agent.total_size_bytes,
                empty_count: agent.empty_count,
                low_value_count: agent.low_value_count,
                valuable_count: agent.valuable_count,
                sessions: agent
                    .sessions
                    .into_iter()
                    .map(|session| SessionAnalysis {
                        agent: session.agent,
                        session_id: session.session_id,
                        file_path: session.file_path,
                        size_bytes: session.size_bytes,
                        message_count: session.message_count,
                        user_message_count: session.user_message_count,
                        assistant_message_count: session.assistant_message_count,
                        last_activity: session.last_activity,
                        age_days: session.age_days,
                        total_tokens: session.total_tokens,
                        model: session.model,
                        category: session.category,
                        kind: session.kind,
                    })
                    .collect(),
            })
            .collect())
    })
}

#[tauri::command]
pub async fn remote_delete_sessions_by_ids(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    agent_id: String,
    session_ids: Vec<String>,
) -> Result<usize, String> {
    timed_async!("remote_delete_sessions_by_ids", {
        if agent_id.trim().is_empty() || agent_id.contains("..") || agent_id.contains('/') {
            return Err("invalid agent id".into());
        }

        let mut deleted = 0usize;
        for sid in &session_ids {
            if sid.contains("..") || sid.contains('/') || sid.contains('\\') {
                continue;
            }
            // Delete from both sessions and sessions_archive
            let cmd = format!(
                "rm -f ~/.openclaw/agents/{agent}/sessions/{sid}.jsonl ~/.openclaw/agents/{agent}/sessions/{sid}-topic-*.jsonl ~/.openclaw/agents/{agent}/sessions_archive/{sid}.jsonl ~/.openclaw/agents/{agent}/sessions_archive/{sid}-topic-*.jsonl 2>/dev/null; echo ok",
                agent = agent_id, sid = sid
            );
            if let Ok(r) = pool.exec(&host_id, &cmd).await {
                if r.stdout.trim() == "ok" {
                    deleted += 1;
                }
            }
        }

        // Clean up sessions.json
        let sessions_json_path = format!("~/.openclaw/agents/{}/sessions/sessions.json", agent_id);
        if let Ok(content) = pool.sftp_read(&host_id, &sessions_json_path).await {
            let ids: Vec<&str> = session_ids.iter().map(String::as_str).collect();
            if let Ok(updated) = clawpal_core::sessions::filter_sessions_by_ids(&content, &ids) {
                let _ = pool
                    .sftp_write(&host_id, &sessions_json_path, &updated)
                    .await;
            }
        }

        Ok(deleted)
    })
}

#[tauri::command]
pub async fn remote_list_session_files(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<SessionFile>, String> {
    timed_async!("remote_list_session_files", {
        let script = r#"
    setopt nonomatch 2>/dev/null; shopt -s nullglob 2>/dev/null
    cd ~/.openclaw/agents 2>/dev/null || { echo "[]"; exit 0; }
    sep=""
    echo "["
    for agent_dir in */; do
      [ -d "$agent_dir" ] || continue
      agent="${agent_dir%/}"
      safe_agent=$(printf '%s' "$agent" | sed 's/\\/\\\\/g; s/"/\\"/g')
      for kind in sessions sessions_archive; do
        dir="$agent_dir$kind"
        [ -d "$dir" ] || continue
        for f in "$dir"/*.jsonl; do
          [ -f "$f" ] || continue
          size=$(wc -c < "$f" 2>/dev/null | tr -d ' ')
          safe_path=$(printf '%s' "$f" | sed 's/\\/\\\\/g; s/"/\\"/g')
          printf '%s{"agent":"%s","kind":"%s","path":"%s","sizeBytes":%s}' "$sep" "$safe_agent" "$kind" "$safe_path" "$size"
          sep=","
        done
      done
    done
    echo "]"
    "#;
        let result = pool.exec(&host_id, script).await?;
        let core = clawpal_core::sessions::parse_session_file_list(result.stdout.trim())?;
        Ok(core
            .into_iter()
            .map(|entry| SessionFile {
                path: entry.path,
                relative_path: entry.relative_path,
                agent: entry.agent,
                kind: entry.kind,
                size_bytes: entry.size_bytes,
            })
            .collect())
    })
}

#[tauri::command]
pub async fn remote_preview_session(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    agent_id: String,
    session_id: String,
) -> Result<Vec<Value>, String> {
    timed_async!("remote_preview_session", {
        if agent_id.contains("..")
            || agent_id.contains('/')
            || session_id.contains("..")
            || session_id.contains('/')
        {
            return Err("invalid id".into());
        }
        let jsonl_name = format!("{}.jsonl", session_id);

        // Try sessions dir first, then archive
        let paths = [
            format!("~/.openclaw/agents/{}/sessions/{}", agent_id, jsonl_name),
            format!(
                "~/.openclaw/agents/{}/sessions_archive/{}",
                agent_id, jsonl_name
            ),
        ];

        let mut content = String::new();
        for path in &paths {
            if let Ok(c) = pool.sftp_read(&host_id, path).await {
                content = c;
                break;
            }
        }
        if content.is_empty() {
            return Ok(Vec::new());
        }

        let parsed = clawpal_core::sessions::parse_session_preview(&content)?;
        Ok(parsed
            .into_iter()
            .map(|m| serde_json::json!({ "role": m.role, "content": m.content }))
            .collect())
    })
}

#[tauri::command]
pub async fn remote_clear_all_sessions(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<usize, String> {
    timed_async!("remote_clear_all_sessions", {
        let script = r#"
    setopt nonomatch 2>/dev/null; shopt -s nullglob 2>/dev/null
    count=0
    cd ~/.openclaw/agents 2>/dev/null || { echo "0"; exit 0; }
    for agent_dir in */; do
      for kind in sessions sessions_archive; do
        dir="$agent_dir$kind"
        [ -d "$dir" ] || continue
        for f in "$dir"/*; do
          [ -f "$f" ] || continue
          rm -f "$f" && count=$((count + 1))
        done
      done
    done
    echo "$count"
    "#;
        let result = pool.exec(&host_id, script).await?;
        let count: usize = result.stdout.trim().parse().unwrap_or(0);
        Ok(count)
    })
}

#[tauri::command]
pub fn list_session_files() -> Result<Vec<SessionFile>, String> {
    timed_sync!("list_session_files", {
        let paths = resolve_paths();
        list_session_files_detailed(&paths.base_dir)
    })
}

#[tauri::command]
pub fn clear_all_sessions() -> Result<usize, String> {
    timed_sync!("clear_all_sessions", {
        let paths = resolve_paths();
        clear_agent_and_global_sessions(&paths.base_dir.join("agents"), None)
    })
}

#[tauri::command]
pub async fn analyze_sessions() -> Result<Vec<AgentSessionAnalysis>, String> {
    timed_async!("analyze_sessions", {
        tauri::async_runtime::spawn_blocking(|| analyze_sessions_sync())
            .await
            .map_err(|e| e.to_string())?
    })
}

#[tauri::command]
pub async fn delete_sessions_by_ids(
    agent_id: String,
    session_ids: Vec<String>,
) -> Result<usize, String> {
    timed_async!("delete_sessions_by_ids", {
        tauri::async_runtime::spawn_blocking(move || {
            delete_sessions_by_ids_sync(&agent_id, &session_ids)
        })
        .await
        .map_err(|e| e.to_string())?
    })
}

#[tauri::command]
pub async fn preview_session(agent_id: String, session_id: String) -> Result<Vec<Value>, String> {
    timed_async!("preview_session", {
        tauri::async_runtime::spawn_blocking(move || preview_session_sync(&agent_id, &session_id))
            .await
            .map_err(|e| e.to_string())?
    })
}
