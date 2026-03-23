use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

static SESSION_STREAM_CANCEL_FLAGS: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionAnalysisChunkPayload {
    handle_id: String,
    agent: String,
    sessions: Vec<SessionAnalysis>,
    total_files: usize,
    total_size_bytes: u64,
    empty_count: usize,
    low_value_count: usize,
    valuable_count: usize,
    done: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionStreamDonePayload {
    handle_id: String,
    total_agents: usize,
    total_sessions: usize,
    cancelled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionStreamErrorPayload {
    handle_id: String,
    error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionPreviewPagePayload {
    handle_id: String,
    page: usize,
    messages: Vec<clawpal_core::sessions::SessionPreviewMessage>,
    total_messages: usize,
}

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

#[tauri::command]
pub fn cancel_stream(handle_id: String) -> Result<bool, String> {
    timed_sync!("cancel_stream", {
        let flag = SESSION_STREAM_CANCEL_FLAGS
            .lock()
            .map_err(|_| "failed to lock session stream registry".to_string())?
            .get(&handle_id)
            .cloned();
        if let Some(flag) = flag {
            flag.store(true, Ordering::Relaxed);
            Ok(true)
        } else {
            Ok(false)
        }
    })
}

#[tauri::command]
pub async fn analyze_sessions_stream(
    app: AppHandle,
    batch_size: Option<usize>,
) -> Result<String, String> {
    timed_async!("analyze_sessions_stream", {
        let batch_size = sanitize_stream_batch_size(batch_size, 50, 250);
        let (handle_id, cancel_flag) = register_session_stream()?;
        let app_handle = app.clone();
        let handle_for_task = handle_id.clone();

        tauri::async_runtime::spawn_blocking(move || {
            let result = stream_local_session_analysis(
                &app_handle,
                &handle_for_task,
                &cancel_flag,
                batch_size,
            );
            finalize_session_stream(&app_handle, &handle_for_task, &cancel_flag, result);
        });

        Ok(handle_id)
    })
}

#[tauri::command]
pub async fn remote_analyze_sessions_stream(
    app: AppHandle,
    host_id: String,
    batch_size: Option<usize>,
) -> Result<String, String> {
    timed_async!("remote_analyze_sessions_stream", {
        let batch_size = sanitize_stream_batch_size(batch_size, 50, 250);
        let (handle_id, cancel_flag) = register_session_stream()?;
        let app_handle = app.clone();
        let handle_for_task = handle_id.clone();
        let host_for_task = host_id.clone();

        tauri::async_runtime::spawn(async move {
            let pool = app_handle.state::<SshConnectionPool>();
            let result = stream_remote_session_analysis(
                &pool,
                &app_handle,
                &handle_for_task,
                &cancel_flag,
                host_for_task,
                batch_size,
            )
            .await;
            finalize_session_stream(&app_handle, &handle_for_task, &cancel_flag, result);
        });

        Ok(handle_id)
    })
}

#[tauri::command]
pub async fn preview_session_stream(
    app: AppHandle,
    agent_id: String,
    session_id: String,
    page_size: Option<usize>,
) -> Result<String, String> {
    timed_async!("preview_session_stream", {
        let page_size = sanitize_stream_batch_size(page_size, 100, 500);
        let (handle_id, cancel_flag) = register_session_stream()?;
        let app_handle = app.clone();
        let handle_for_task = handle_id.clone();

        tauri::async_runtime::spawn_blocking(move || {
            let result = stream_local_session_preview(
                &app_handle,
                &handle_for_task,
                &cancel_flag,
                &agent_id,
                &session_id,
                page_size,
            );
            finalize_preview_stream(&app_handle, &handle_for_task, &cancel_flag, result);
        });

        Ok(handle_id)
    })
}

#[tauri::command]
pub async fn remote_preview_session_stream(
    app: AppHandle,
    host_id: String,
    agent_id: String,
    session_id: String,
    page_size: Option<usize>,
) -> Result<String, String> {
    timed_async!("remote_preview_session_stream", {
        let page_size = sanitize_stream_batch_size(page_size, 100, 500);
        let (handle_id, cancel_flag) = register_session_stream()?;
        let app_handle = app.clone();
        let handle_for_task = handle_id.clone();
        let host_for_task = host_id.clone();

        tauri::async_runtime::spawn(async move {
            let pool = app_handle.state::<SshConnectionPool>();
            let result = stream_remote_session_preview(
                &pool,
                &app_handle,
                &handle_for_task,
                &cancel_flag,
                host_for_task,
                agent_id,
                session_id,
                page_size,
            )
            .await;
            finalize_preview_stream(&app_handle, &handle_for_task, &cancel_flag, result);
        });

        Ok(handle_id)
    })
}

fn sanitize_stream_batch_size(value: Option<usize>, default: usize, max: usize) -> usize {
    value.unwrap_or(default).clamp(1, max)
}

fn register_session_stream() -> Result<(String, Arc<AtomicBool>), String> {
    let handle_id = uuid::Uuid::new_v4().to_string();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    SESSION_STREAM_CANCEL_FLAGS
        .lock()
        .map_err(|_| "failed to lock session stream registry".to_string())?
        .insert(handle_id.clone(), cancel_flag.clone());
    Ok((handle_id, cancel_flag))
}

fn unregister_session_stream(handle_id: &str) {
    if let Ok(mut guard) = SESSION_STREAM_CANCEL_FLAGS.lock() {
        guard.remove(handle_id);
    }
}

fn stream_cancelled(cancel_flag: &Arc<AtomicBool>) -> bool {
    cancel_flag.load(Ordering::Relaxed)
}

fn emit_session_stream_error(app: &AppHandle, handle_id: &str, error: String) {
    let _ = app.emit(
        "sessions:error",
        SessionStreamErrorPayload {
            handle_id: handle_id.to_string(),
            error,
        },
    );
}

fn emit_session_done(
    app: &AppHandle,
    handle_id: &str,
    total_agents: usize,
    total_sessions: usize,
    cancelled: bool,
) {
    let _ = app.emit(
        "sessions:done",
        SessionStreamDonePayload {
            handle_id: handle_id.to_string(),
            total_agents,
            total_sessions,
            cancelled,
        },
    );
}

fn emit_preview_done(app: &AppHandle, handle_id: &str, total_messages: usize, cancelled: bool) {
    let _ = app.emit(
        "session-preview:done",
        serde_json::json!({
            "handleId": handle_id,
            "totalMessages": total_messages,
            "cancelled": cancelled,
        }),
    );
}

fn finalize_session_stream(
    app: &AppHandle,
    handle_id: &str,
    cancel_flag: &Arc<AtomicBool>,
    result: Result<(usize, usize), String>,
) {
    let cancelled = stream_cancelled(cancel_flag);
    match result {
        Ok((total_agents, total_sessions)) => {
            emit_session_done(app, handle_id, total_agents, total_sessions, cancelled);
        }
        Err(error) => {
            emit_session_stream_error(app, handle_id, error);
            emit_session_done(app, handle_id, 0, 0, cancelled);
        }
    }
    unregister_session_stream(handle_id);
}

fn finalize_preview_stream(
    app: &AppHandle,
    handle_id: &str,
    cancel_flag: &Arc<AtomicBool>,
    result: Result<usize, String>,
) {
    let cancelled = stream_cancelled(cancel_flag);
    match result {
        Ok(total_messages) => emit_preview_done(app, handle_id, total_messages, cancelled),
        Err(error) => {
            emit_session_stream_error(app, handle_id, error);
            emit_preview_done(app, handle_id, 0, cancelled);
        }
    }
    unregister_session_stream(handle_id);
}

fn emit_analysis_chunk(
    app: &AppHandle,
    handle_id: &str,
    payload: SessionAnalysisChunkPayload,
) -> Result<(), String> {
    app.emit("sessions:chunk", payload)
        .map_err(|e| format!("failed to emit sessions:chunk for {handle_id}: {e}"))
}

fn emit_preview_page(
    app: &AppHandle,
    handle_id: &str,
    payload: SessionPreviewPagePayload,
) -> Result<(), String> {
    app.emit("session-preview:page", payload)
        .map_err(|e| format!("failed to emit session-preview:page for {handle_id}: {e}"))
}

fn core_session_to_tauri(session: clawpal_core::sessions::SessionAnalysis) -> SessionAnalysis {
    SessionAnalysis {
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
    }
}

fn build_local_session_analysis(
    agent: &str,
    file_path: &Path,
    metadata: &fs::Metadata,
    meta_by_id: &HashMap<String, &Value>,
    now_ms: f64,
    kind_name: &str,
) -> Result<SessionAnalysis, String> {
    let size_bytes = metadata.len();
    let fname = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid session file path: {}", file_path.display()))?;
    let session_id = fname.trim_end_matches(".jsonl").to_string();

    let mut message_count = 0usize;
    let mut user_message_count = 0usize;
    let mut assistant_message_count = 0usize;
    let mut last_activity: Option<String> = None;

    if let Ok(file) = fs::File::open(file_path) {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(_) => continue,
            };
            if line.trim().is_empty() {
                continue;
            }
            let obj: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if obj.get("type").and_then(Value::as_str) == Some("message") {
                message_count += 1;
                if let Some(ts) = obj.get("timestamp").and_then(Value::as_str) {
                    last_activity = Some(ts.to_string());
                }
                match obj.pointer("/message/role").and_then(Value::as_str) {
                    Some("user") => user_message_count += 1,
                    Some("assistant") => assistant_message_count += 1,
                    _ => {}
                }
            }
        }
    }

    let base_id = if session_id.contains("-topic-") {
        session_id.split("-topic-").next().unwrap_or(&session_id)
    } else {
        &session_id
    };
    let meta = meta_by_id.get(base_id);

    let total_tokens = meta
        .and_then(|m| m.get("totalTokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let model = meta
        .and_then(|m| m.get("model"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let updated_at = meta
        .and_then(|m| m.get("updatedAt"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);

    let age_days = if updated_at > 0.0 {
        (now_ms - updated_at) / (1000.0 * 60.0 * 60.0 * 24.0)
    } else {
        metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| (now_ms - d.as_millis() as f64) / (1000.0 * 60.0 * 60.0 * 24.0))
            .unwrap_or(0.0)
    };

    Ok(SessionAnalysis {
        agent: agent.to_string(),
        session_id,
        file_path: file_path.to_string_lossy().to_string(),
        size_bytes,
        message_count,
        user_message_count,
        assistant_message_count,
        last_activity,
        age_days,
        total_tokens,
        model,
        category: clawpal_core::sessions::classify_session(
            size_bytes,
            message_count,
            user_message_count,
            age_days,
        )
        .to_string(),
        kind: kind_name.to_string(),
    })
}

fn stream_local_session_analysis(
    app: &AppHandle,
    handle_id: &str,
    cancel_flag: &Arc<AtomicBool>,
    batch_size: usize,
) -> Result<(usize, usize), String> {
    let paths = resolve_paths();
    let agents_root = paths.base_dir.join("agents");
    if !agents_root.exists() {
        return Ok((0, 0));
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64;

    let mut total_agents = 0usize;
    let mut total_sessions = 0usize;
    let entries = fs::read_dir(&agents_root).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        if stream_cancelled(cancel_flag) {
            break;
        }

        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        let agent = entry.file_name().to_string_lossy().to_string();

        let sessions_json_path = entry_path.join("sessions").join("sessions.json");
        let sessions_meta: HashMap<String, Value> = if sessions_json_path.exists() {
            let text = fs::read_to_string(&sessions_json_path).unwrap_or_default();
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            HashMap::new()
        };
        let mut meta_by_id: HashMap<String, &Value> = HashMap::new();
        for value in sessions_meta.values() {
            if let Some(session_id) = value.get("sessionId").and_then(Value::as_str) {
                meta_by_id.insert(session_id.to_string(), value);
            }
        }

        let mut batch = Vec::new();
        let mut total_files = 0usize;
        let mut total_size_bytes = 0u64;
        let mut empty_count = 0usize;
        let mut low_value_count = 0usize;
        let mut valuable_count = 0usize;

        for (kind_name, dir_name) in [("sessions", "sessions"), ("archive", "sessions_archive")] {
            let dir = entry_path.join(dir_name);
            if !dir.exists() {
                continue;
            }
            let files = match fs::read_dir(&dir) {
                Ok(files) => files,
                Err(_) => continue,
            };
            for file_entry in files.flatten() {
                if stream_cancelled(cancel_flag) {
                    break;
                }

                let file_path = file_entry.path();
                let file_name = file_entry.file_name().to_string_lossy().to_string();
                if !file_name.ends_with(".jsonl") {
                    continue;
                }
                let metadata = match file_entry.metadata() {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };
                let session = build_local_session_analysis(
                    &agent,
                    &file_path,
                    &metadata,
                    &meta_by_id,
                    now_ms,
                    kind_name,
                )?;

                total_files += 1;
                total_size_bytes = total_size_bytes.saturating_add(session.size_bytes);
                match session.category.as_str() {
                    "empty" => empty_count += 1,
                    "low_value" => low_value_count += 1,
                    _ => valuable_count += 1,
                }
                batch.push(session);

                if batch.len() >= batch_size {
                    emit_analysis_chunk(
                        app,
                        handle_id,
                        SessionAnalysisChunkPayload {
                            handle_id: handle_id.to_string(),
                            agent: agent.clone(),
                            sessions: std::mem::take(&mut batch),
                            total_files,
                            total_size_bytes,
                            empty_count,
                            low_value_count,
                            valuable_count,
                            done: false,
                        },
                    )?;
                }
            }
        }

        if total_files == 0 {
            continue;
        }

        total_agents += 1;
        total_sessions = total_sessions.saturating_add(total_files);
        emit_analysis_chunk(
            app,
            handle_id,
            SessionAnalysisChunkPayload {
                handle_id: handle_id.to_string(),
                agent: agent.clone(),
                sessions: std::mem::take(&mut batch),
                total_files,
                total_size_bytes,
                empty_count,
                low_value_count,
                valuable_count,
                done: true,
            },
        )?;
    }

    Ok((total_agents, total_sessions))
}

async fn list_remote_agents(
    pool: &SshConnectionPool,
    host_id: &str,
) -> Result<Vec<String>, String> {
    let result = pool
        .exec(
            host_id,
            r#"
setopt nonomatch 2>/dev/null; shopt -s nullglob 2>/dev/null
cd ~/.openclaw/agents 2>/dev/null || exit 0
for agent_dir in */; do
  [ -d "$agent_dir" ] || continue
  printf '%s\n' "${agent_dir%/}"
done
"#,
        )
        .await?;
    Ok(result
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn build_remote_agent_analysis_script(agent: &str) -> String {
    let escaped_agent = shell_escape(agent);
    format!(
        r#"
setopt nonomatch 2>/dev/null; shopt -s nullglob 2>/dev/null
agent={escaped_agent}
agent_root="$HOME/.openclaw/agents/$agent"
[ -d "$agent_root" ] || exit 0
now=$(date +%s)
safe_agent=$(printf '%s' "$agent" | sed 's/\\/\\\\/g; s/"/\\"/g')
for kind in sessions sessions_archive; do
  dir="$agent_root/$kind"
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
    printf '{{"agent":"%s","sessionId":"%s","sizeBytes":%s,"messageCount":%s,"userMessageCount":%s,"assistantMessageCount":%s,"ageDays":%s,"kind":"%s"}}\n' \
      "$safe_agent" "$safe_fname" "$size" "$msgs" "$user_msgs" "$asst_msgs" "$age_days" "$kind"
  done
done
"#
    )
}

async fn stream_remote_session_analysis(
    pool: &SshConnectionPool,
    app: &AppHandle,
    handle_id: &str,
    cancel_flag: &Arc<AtomicBool>,
    host_id: String,
    batch_size: usize,
) -> Result<(usize, usize), String> {
    let agents = list_remote_agents(pool, &host_id).await?;
    let mut total_agents = 0usize;
    let mut total_sessions = 0usize;

    for agent in agents {
        if stream_cancelled(cancel_flag) {
            break;
        }

        let (mut rx, join) = pool
            .exec_streaming(&host_id, &build_remote_agent_analysis_script(&agent))
            .await?;

        let mut batch = Vec::new();
        let mut total_files = 0usize;
        let mut total_size_bytes = 0u64;
        let mut empty_count = 0usize;
        let mut low_value_count = 0usize;
        let mut valuable_count = 0usize;

        while let Some(line) = rx.recv().await {
            if stream_cancelled(cancel_flag) {
                break;
            }
            let Some(session) = clawpal_core::sessions::parse_session_analysis_entry_line(&line)?
            else {
                continue;
            };
            let session = core_session_to_tauri(session);
            total_files += 1;
            total_size_bytes = total_size_bytes.saturating_add(session.size_bytes);
            match session.category.as_str() {
                "empty" => empty_count += 1,
                "low_value" => low_value_count += 1,
                _ => valuable_count += 1,
            }
            batch.push(session);

            if batch.len() >= batch_size {
                emit_analysis_chunk(
                    app,
                    handle_id,
                    SessionAnalysisChunkPayload {
                        handle_id: handle_id.to_string(),
                        agent: agent.clone(),
                        sessions: std::mem::take(&mut batch),
                        total_files,
                        total_size_bytes,
                        empty_count,
                        low_value_count,
                        valuable_count,
                        done: false,
                    },
                )?;
            }
        }

        // Await the SSH command completion and check for errors
        match join.await {
            Ok(Ok((exit_code, stderr))) => {
                if exit_code != 0 && total_files == 0 {
                    crate::commands::logs::log_dev(format!(
                        "[dev][session_stream] remote analysis for agent {} exited with code {}: {}",
                        agent, exit_code, stderr
                    ));
                }
            }
            Ok(Err(e)) => {
                crate::commands::logs::log_dev(format!(
                    "[dev][session_stream] remote analysis SSH error for agent {}: {}",
                    agent, e
                ));
            }
            Err(e) => {
                crate::commands::logs::log_dev(format!(
                    "[dev][session_stream] remote analysis join error for agent {}: {}",
                    agent, e
                ));
            }
        }

        if total_files == 0 {
            continue;
        }

        total_agents += 1;
        total_sessions = total_sessions.saturating_add(total_files);
        emit_analysis_chunk(
            app,
            handle_id,
            SessionAnalysisChunkPayload {
                handle_id: handle_id.to_string(),
                agent: agent.clone(),
                sessions: std::mem::take(&mut batch),
                total_files,
                total_size_bytes,
                empty_count,
                low_value_count,
                valuable_count,
                done: true,
            },
        )?;
    }

    Ok((total_agents, total_sessions))
}

fn validate_session_stream_ids(agent_id: &str, session_id: &str) -> Result<(), String> {
    if agent_id.contains("..") || agent_id.contains('/') || agent_id.contains('\\') {
        return Err("invalid agent id".into());
    }
    if session_id.contains("..") || session_id.contains('/') || session_id.contains('\\') {
        return Err("invalid session id".into());
    }
    Ok(())
}

fn resolve_local_session_file(agent_id: &str, session_id: &str) -> Result<Option<PathBuf>, String> {
    validate_session_stream_ids(agent_id, session_id)?;
    let paths = resolve_paths();
    let agent_dir = paths.base_dir.join("agents").join(agent_id);
    let jsonl_name = format!("{session_id}.jsonl");
    Ok(["sessions", "sessions_archive"]
        .iter()
        .map(|dir| agent_dir.join(dir).join(&jsonl_name))
        .find(|path| path.exists()))
}

fn stream_local_session_preview(
    app: &AppHandle,
    handle_id: &str,
    cancel_flag: &Arc<AtomicBool>,
    agent_id: &str,
    session_id: &str,
    page_size: usize,
) -> Result<usize, String> {
    let Some(file_path) = resolve_local_session_file(agent_id, session_id)? else {
        return Ok(0);
    };

    let file = fs::File::open(&file_path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut total_messages = 0usize;
    let mut page = 0usize;

    for line in reader.lines() {
        if stream_cancelled(cancel_flag) {
            break;
        }
        let line = match line {
            Ok(line) => line,
            Err(_) => continue,
        };
        if let Some(message) = clawpal_core::sessions::parse_session_preview_line(&line)? {
            messages.push(message);
            total_messages += 1;
            if messages.len() >= page_size {
                page += 1;
                emit_preview_page(
                    app,
                    handle_id,
                    SessionPreviewPagePayload {
                        handle_id: handle_id.to_string(),
                        page,
                        messages: std::mem::take(&mut messages),
                        total_messages,
                    },
                )?;
            }
        }
    }

    if !messages.is_empty() {
        page += 1;
        emit_preview_page(
            app,
            handle_id,
            SessionPreviewPagePayload {
                handle_id: handle_id.to_string(),
                page,
                messages,
                total_messages,
            },
        )?;
    }

    Ok(total_messages)
}

async fn resolve_remote_session_file(
    pool: &SshConnectionPool,
    host_id: &str,
    agent_id: &str,
    session_id: &str,
) -> Result<Option<String>, String> {
    validate_session_stream_ids(agent_id, session_id)?;
    let agent = shell_escape(agent_id);
    let session = shell_escape(&format!("{session_id}.jsonl"));
    let command = format!(
        r#"
agent={agent}
session={session}
for path in "$HOME/.openclaw/agents/$agent/sessions/$session" "$HOME/.openclaw/agents/$agent/sessions_archive/$session"; do
  if [ -f "$path" ]; then
    printf '%s\n' "$path"
    break
  fi
done
"#
    );
    let result = pool.exec_login(host_id, &command).await?;
    Ok(result
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned))
}

async fn stream_remote_session_preview(
    pool: &SshConnectionPool,
    app: &AppHandle,
    handle_id: &str,
    cancel_flag: &Arc<AtomicBool>,
    host_id: String,
    agent_id: String,
    session_id: String,
    page_size: usize,
) -> Result<usize, String> {
    let Some(remote_path) =
        resolve_remote_session_file(pool, &host_id, &agent_id, &session_id).await?
    else {
        return Ok(0);
    };

    let escaped_path = shell_escape(&remote_path);
    let mut total_messages = 0usize;
    let mut page = 0usize;
    let mut start_line = 1usize;

    loop {
        if stream_cancelled(cancel_flag) {
            break;
        }

        let end_line = start_line + page_size;
        let command = format!(
            "awk 'NR >= {start} && NR < {end} {{ print }}' {path}",
            start = start_line,
            end = end_line,
            path = escaped_path
        );
        let result = pool.exec_login(&host_id, &command).await?;
        if result.exit_code != 0 {
            return Err(format!(
                "Remote preview failed (exit {}): {}",
                result.exit_code, result.stderr
            ));
        }

        let raw_lines: Vec<&str> = result.stdout.lines().collect();
        if raw_lines.is_empty() {
            break;
        }

        let mut messages = Vec::new();
        for line in &raw_lines {
            if let Some(message) = clawpal_core::sessions::parse_session_preview_line(line)? {
                total_messages += 1;
                messages.push(message);
            }
        }

        if !messages.is_empty() {
            page += 1;
            emit_preview_page(
                app,
                handle_id,
                SessionPreviewPagePayload {
                    handle_id: handle_id.to_string(),
                    page,
                    messages,
                    total_messages,
                },
            )?;
        }

        if raw_lines.len() < page_size {
            break;
        }
        start_line += page_size;
    }

    Ok(total_messages)
}

// --- Extracted from mod.rs ---

pub(crate) fn analyze_sessions_sync() -> Result<Vec<AgentSessionAnalysis>, String> {
    let paths = resolve_paths();
    let agents_root = paths.base_dir.join("agents");
    if !agents_root.exists() {
        return Ok(Vec::new());
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64;

    let mut results: Vec<AgentSessionAnalysis> = Vec::new();
    let entries = fs::read_dir(&agents_root).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        let agent = entry.file_name().to_string_lossy().to_string();

        // Load sessions.json metadata for this agent
        let sessions_json_path = entry_path.join("sessions").join("sessions.json");
        let sessions_meta: HashMap<String, Value> = if sessions_json_path.exists() {
            let text = fs::read_to_string(&sessions_json_path).unwrap_or_default();
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            HashMap::new()
        };

        // Build sessionId -> metadata lookup
        let mut meta_by_id: HashMap<String, &Value> = HashMap::new();
        for (_key, val) in &sessions_meta {
            if let Some(sid) = val.get("sessionId").and_then(Value::as_str) {
                meta_by_id.insert(sid.to_string(), val);
            }
        }

        let mut agent_sessions: Vec<SessionAnalysis> = Vec::new();

        for (kind_name, dir_name) in [("sessions", "sessions"), ("archive", "sessions_archive")] {
            let dir = entry_path.join(dir_name);
            if !dir.exists() {
                continue;
            }
            let files = match fs::read_dir(&dir) {
                Ok(f) => f,
                Err(_) => continue,
            };
            for file_entry in files.flatten() {
                let file_path = file_entry.path();
                let fname = file_entry.file_name().to_string_lossy().to_string();
                if !fname.ends_with(".jsonl") {
                    continue;
                }

                let metadata = match file_entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let size_bytes = metadata.len();

                // Extract session ID from filename (e.g. "abc123.jsonl" or "abc123-topic-456.jsonl")
                let session_id = fname.trim_end_matches(".jsonl").to_string();

                // Parse JSONL to count messages
                let mut message_count = 0usize;
                let mut user_message_count = 0usize;
                let mut assistant_message_count = 0usize;
                let mut last_activity: Option<String> = None;

                if let Ok(file) = fs::File::open(&file_path) {
                    let reader = BufReader::new(file);
                    for line in reader.lines() {
                        let line = match line {
                            Ok(l) => l,
                            Err(_) => continue,
                        };
                        if line.trim().is_empty() {
                            continue;
                        }
                        let obj: Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        if obj.get("type").and_then(Value::as_str) == Some("message") {
                            message_count += 1;
                            if let Some(ts) = obj.get("timestamp").and_then(Value::as_str) {
                                last_activity = Some(ts.to_string());
                            }
                            let role = obj.pointer("/message/role").and_then(Value::as_str);
                            match role {
                                Some("user") => user_message_count += 1,
                                Some("assistant") => assistant_message_count += 1,
                                _ => {}
                            }
                        }
                    }
                }

                // Look up metadata from sessions.json
                // For topic files like "abc-topic-123", try the base session ID "abc"
                let base_id = if session_id.contains("-topic-") {
                    session_id.split("-topic-").next().unwrap_or(&session_id)
                } else {
                    &session_id
                };
                let meta = meta_by_id.get(base_id);

                let total_tokens = meta
                    .and_then(|m| m.get("totalTokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let model = meta
                    .and_then(|m| m.get("model"))
                    .and_then(Value::as_str)
                    .map(|s| s.to_string());
                let updated_at = meta
                    .and_then(|m| m.get("updatedAt"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);

                let age_days = if updated_at > 0.0 {
                    (now - updated_at) / (1000.0 * 60.0 * 60.0 * 24.0)
                } else {
                    // Fall back to file modification time
                    metadata
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| (now - d.as_millis() as f64) / (1000.0 * 60.0 * 60.0 * 24.0))
                        .unwrap_or(0.0)
                };

                // Classify
                let category = if size_bytes < 500 || message_count == 0 {
                    "empty"
                } else if user_message_count <= 1 && age_days > 7.0 {
                    "low_value"
                } else {
                    "valuable"
                };

                agent_sessions.push(SessionAnalysis {
                    agent: agent.clone(),
                    session_id,
                    file_path: file_path.to_string_lossy().to_string(),
                    size_bytes,
                    message_count,
                    user_message_count,
                    assistant_message_count,
                    last_activity,
                    age_days,
                    total_tokens,
                    model,
                    category: category.to_string(),
                    kind: kind_name.to_string(),
                });
            }
        }

        // Sort: empty first, then low_value, then valuable; within each by age descending
        agent_sessions.sort_by(|a, b| {
            let cat_order = |c: &str| match c {
                "empty" => 0,
                "low_value" => 1,
                _ => 2,
            };
            cat_order(&a.category).cmp(&cat_order(&b.category)).then(
                b.age_days
                    .partial_cmp(&a.age_days)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });

        let total_files = agent_sessions.len();
        let total_size_bytes = agent_sessions.iter().map(|s| s.size_bytes).sum();
        let empty_count = agent_sessions
            .iter()
            .filter(|s| s.category == "empty")
            .count();
        let low_value_count = agent_sessions
            .iter()
            .filter(|s| s.category == "low_value")
            .count();
        let valuable_count = agent_sessions
            .iter()
            .filter(|s| s.category == "valuable")
            .count();

        if total_files > 0 {
            results.push(AgentSessionAnalysis {
                agent,
                total_files,
                total_size_bytes,
                empty_count,
                low_value_count,
                valuable_count,
                sessions: agent_sessions,
            });
        }
    }

    results.sort_by(|a, b| b.total_size_bytes.cmp(&a.total_size_bytes));
    Ok(results)
}

pub(crate) fn delete_sessions_by_ids_sync(
    agent_id: &str,
    session_ids: &[String],
) -> Result<usize, String> {
    if agent_id.trim().is_empty() {
        return Err("agent id is required".into());
    }
    if agent_id.contains("..") || agent_id.contains('/') || agent_id.contains('\\') {
        return Err("invalid agent id".into());
    }
    let paths = resolve_paths();
    let agent_dir = paths.base_dir.join("agents").join(agent_id);

    let mut deleted = 0usize;

    // Search in both sessions and sessions_archive
    let dirs = ["sessions", "sessions_archive"];

    for sid in session_ids {
        if sid.contains("..") || sid.contains('/') || sid.contains('\\') {
            continue;
        }
        for dir_name in &dirs {
            let dir = agent_dir.join(dir_name);
            if !dir.exists() {
                continue;
            }
            let jsonl_path = dir.join(format!("{}.jsonl", sid));
            if jsonl_path.exists() {
                if fs::remove_file(&jsonl_path).is_ok() {
                    deleted += 1;
                }
            }
            // Also clean up related files (topic files, .lock, .deleted.*)
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    if fname.starts_with(sid.as_str()) && fname != format!("{}.jsonl", sid) {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
    }

    // Remove entries from sessions.json (in sessions dir)
    let sessions_json_path = agent_dir.join("sessions").join("sessions.json");
    if sessions_json_path.exists() {
        if let Ok(text) = fs::read_to_string(&sessions_json_path) {
            if let Ok(mut data) = serde_json::from_str::<serde_json::Map<String, Value>>(&text) {
                let id_set: HashSet<&str> = session_ids.iter().map(String::as_str).collect();
                data.retain(|_key, val| {
                    let sid = val.get("sessionId").and_then(Value::as_str).unwrap_or("");
                    !id_set.contains(sid)
                });
                let _ = fs::write(
                    &sessions_json_path,
                    serde_json::to_string(&data).unwrap_or_default(),
                );
            }
        }
    }

    Ok(deleted)
}

pub(crate) fn preview_session_sync(agent_id: &str, session_id: &str) -> Result<Vec<Value>, String> {
    if agent_id.contains("..") || agent_id.contains('/') || agent_id.contains('\\') {
        return Err("invalid agent id".into());
    }
    if session_id.contains("..") || session_id.contains('/') || session_id.contains('\\') {
        return Err("invalid session id".into());
    }
    let paths = resolve_paths();
    let agent_dir = paths.base_dir.join("agents").join(agent_id);
    let jsonl_name = format!("{}.jsonl", session_id);

    // Search in both sessions and sessions_archive
    let file_path = ["sessions", "sessions_archive"]
        .iter()
        .map(|dir| agent_dir.join(dir).join(&jsonl_name))
        .find(|p| p.exists());

    let file_path = match file_path {
        Some(p) => p,
        None => return Ok(Vec::new()),
    };

    let file = fs::File::open(&file_path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut messages: Vec<Value> = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if line.trim().is_empty() {
            continue;
        }
        let obj: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if obj.get("type").and_then(Value::as_str) == Some("message") {
            let role = obj
                .pointer("/message/role")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let content = obj
                .pointer("/message/content")
                .map(|c| {
                    if let Some(arr) = c.as_array() {
                        arr.iter()
                            .filter_map(|item| item.get("text").and_then(Value::as_str))
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else if let Some(s) = c.as_str() {
                        s.to_string()
                    } else {
                        String::new()
                    }
                })
                .unwrap_or_default();
            messages.push(serde_json::json!({
                "role": role,
                "content": content,
            }));
        }
    }

    Ok(messages)
}

pub(crate) fn collect_file_inventory(path: &Path, max_files: Option<usize>) -> MemorySummary {
    let mut queue = VecDeque::new();
    let mut file_count = 0usize;
    let mut total_bytes = 0u64;
    let mut files = Vec::new();

    if !path.exists() {
        return MemorySummary {
            file_count: 0,
            total_bytes: 0,
            files,
        };
    }

    queue.push_back(path.to_path_buf());
    while let Some(current) = queue.pop_front() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    queue.push_back(entry_path);
                    continue;
                }
                if metadata.is_file() {
                    file_count += 1;
                    total_bytes = total_bytes.saturating_add(metadata.len());
                    if max_files.is_none_or(|limit| files.len() < limit) {
                        files.push(MemoryFileSummary {
                            path: entry_path.to_string_lossy().to_string(),
                            size_bytes: metadata.len(),
                        });
                    }
                }
            }
        }
    }

    files.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    MemorySummary {
        file_count,
        total_bytes,
        files,
    }
}

pub(crate) fn collect_session_overview(base_dir: &Path) -> SessionSummary {
    let agents_dir = base_dir.join("agents");
    let mut by_agent = Vec::new();
    let mut total_session_files = 0usize;
    let mut total_archive_files = 0usize;
    let mut total_bytes = 0u64;

    if !agents_dir.exists() {
        return SessionSummary {
            total_session_files,
            total_archive_files,
            total_bytes,
            by_agent,
        };
    }

    if let Ok(entries) = fs::read_dir(agents_dir) {
        for entry in entries.flatten() {
            let agent_path = entry.path();
            if !agent_path.is_dir() {
                continue;
            }
            let agent = entry.file_name().to_string_lossy().to_string();
            let sessions_dir = agent_path.join("sessions");
            let archive_dir = agent_path.join("sessions_archive");

            let session_info = collect_file_inventory_with_limit(&sessions_dir);
            let archive_info = collect_file_inventory_with_limit(&archive_dir);

            if session_info.files > 0 || archive_info.files > 0 {
                by_agent.push(AgentSessionSummary {
                    agent: agent.clone(),
                    session_files: session_info.files,
                    archive_files: archive_info.files,
                    total_bytes: session_info
                        .total_bytes
                        .saturating_add(archive_info.total_bytes),
                });
            }

            total_session_files = total_session_files.saturating_add(session_info.files);
            total_archive_files = total_archive_files.saturating_add(archive_info.files);
            total_bytes = total_bytes
                .saturating_add(session_info.total_bytes)
                .saturating_add(archive_info.total_bytes);
        }
    }

    by_agent.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    SessionSummary {
        total_session_files,
        total_archive_files,
        total_bytes,
        by_agent,
    }
}

pub(crate) struct InventorySummary {
    files: usize,
    total_bytes: u64,
}

pub(crate) fn collect_file_inventory_with_limit(path: &Path) -> InventorySummary {
    if !path.exists() {
        return InventorySummary {
            files: 0,
            total_bytes: 0,
        };
    }
    let mut queue = VecDeque::new();
    let mut files = 0usize;
    let mut total_bytes = 0u64;
    queue.push_back(path.to_path_buf());
    while let Some(current) = queue.pop_front() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                let p = entry.path();
                if metadata.is_dir() {
                    queue.push_back(p);
                } else if metadata.is_file() {
                    files += 1;
                    total_bytes = total_bytes.saturating_add(metadata.len());
                }
            }
        }
    }
    InventorySummary { files, total_bytes }
}

pub(crate) fn list_session_files_detailed(base_dir: &Path) -> Result<Vec<SessionFile>, String> {
    let agents_root = base_dir.join("agents");
    if !agents_root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = fs::read_dir(&agents_root).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        let agent = entry.file_name().to_string_lossy().to_string();
        let sessions_root = entry_path.join("sessions");
        let archive_root = entry_path.join("sessions_archive");

        collect_session_files_in_scope(&sessions_root, &agent, "sessions", base_dir, &mut out)?;
        collect_session_files_in_scope(&archive_root, &agent, "archive", base_dir, &mut out)?;
    }
    out.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(out)
}

pub(crate) fn collect_session_files_in_scope(
    scope_root: &Path,
    agent: &str,
    kind: &str,
    base_dir: &Path,
    out: &mut Vec<SessionFile>,
) -> Result<(), String> {
    if !scope_root.exists() {
        return Ok(());
    }
    let mut queue = VecDeque::new();
    queue.push_back(scope_root.to_path_buf());
    while let Some(current) = queue.pop_front() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let metadata = match entry.metadata() {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            if metadata.is_dir() {
                queue.push_back(entry_path);
                continue;
            }
            if metadata.is_file() {
                let relative_path = entry_path
                    .strip_prefix(base_dir)
                    .unwrap_or(&entry_path)
                    .to_string_lossy()
                    .to_string();
                out.push(SessionFile {
                    path: entry_path.to_string_lossy().to_string(),
                    relative_path,
                    agent: agent.to_string(),
                    kind: kind.to_string(),
                    size_bytes: metadata.len(),
                });
            }
        }
    }
    Ok(())
}

pub(crate) fn clear_agent_and_global_sessions(
    agents_root: &Path,
    agent_id: Option<&str>,
) -> Result<usize, String> {
    if !agents_root.exists() {
        return Ok(0);
    }
    let mut total = 0usize;
    let mut targets = Vec::new();

    match agent_id {
        Some(agent) => targets.push(agents_root.join(agent)),
        None => {
            for entry in fs::read_dir(agents_root).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                    targets.push(entry.path());
                }
            }
        }
    }

    for agent_path in targets {
        let sessions = agent_path.join("sessions");
        let archive = agent_path.join("sessions_archive");
        total = total.saturating_add(clear_directory_contents(&sessions)?);
        total = total.saturating_add(clear_directory_contents(&archive)?);
        fs::create_dir_all(&sessions).map_err(|e| e.to_string())?;
        fs::create_dir_all(&archive).map_err(|e| e.to_string())?;
    }
    Ok(total)
}

pub(crate) fn clear_directory_contents(target: &Path) -> Result<usize, String> {
    if !target.exists() {
        return Ok(0);
    }
    let mut total = 0usize;
    let entries = fs::read_dir(target).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        if metadata.is_dir() {
            total = total.saturating_add(clear_directory_contents(&path)?);
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        if metadata.is_file() || metadata.is_symlink() {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
            total = total.saturating_add(1);
        }
    }
    Ok(total)
}
