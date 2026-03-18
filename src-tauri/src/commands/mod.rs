/// Macro for wrapping synchronous command bodies with timing.
/// Uses a closure to capture `?` early-returns so timing is always recorded.
macro_rules! timed_sync {
    ($name:expr, $body:block) => {{
        let __start = std::time::Instant::now();
        let __result = (|| $body)();
        let __elapsed_ms = __start.elapsed().as_millis() as u64;
        crate::commands::perf::record_timing($name, __elapsed_ms);
        __result
    }};
}

/// Macro for wrapping async command bodies with timing.
/// Uses an async block to capture `?` early-returns so timing is always recorded.
macro_rules! timed_async {
    ($name:expr, $body:block) => {{
        let __start = std::time::Instant::now();
        let __result = async $body.await;
        let __elapsed_ms = __start.elapsed().as_millis() as u64;
        crate::commands::perf::record_timing($name, __elapsed_ms);
        __result
    }};
}

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::{
    fs,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::access_discovery::probe_engine::{build_probe_plan_for_local, run_probe_with_redaction};
use crate::access_discovery::store::AccessDiscoveryStore;
use crate::access_discovery::types::{CapabilityProfile, ExecutionExperience};
use crate::config_io::{ensure_dirs, read_openclaw_config, write_json, write_text};
use crate::doctor::{apply_auto_fixes, run_doctor, DoctorReport};
use crate::history::{add_snapshot, list_snapshots, read_snapshot};
use crate::install::session_store::InstallSessionStore;
use crate::install::types::InstallState;
use crate::models::resolve_paths;
use crate::openclaw_doc_resolver::{
    resolve_local_doc_guidance, resolve_remote_doc_guidance, DocCitation, DocGuidance,
    DocResolveIssue, DocResolveRequest, RootCauseHypothesis,
};
use crate::ssh::{SftpEntry, SshConnectionPool, SshExecResult, SshHostConfig, SshTransferStats};
use clawpal_core::ssh::diagnostic::{
    from_any_error, SshDiagnosticReport, SshDiagnosticStatus, SshErrorCode, SshIntent, SshStage,
};

pub mod types;
pub mod cli;
pub mod version;
pub mod discord;
pub mod channels;
pub mod credentials;

pub mod agent;
pub mod app_logs;
pub mod backup;
pub mod config;
pub mod cron;
pub mod discover_local;
pub mod discovery;
pub mod doctor;
pub mod doctor_assistant;
pub mod gateway;
pub mod instance;
pub mod logs;
pub mod model;
pub mod overview;
pub mod perf;
pub mod precheck;
pub mod preferences;
pub mod profiles;
pub mod recipe_cmds;
pub mod rescue;
pub mod sessions;
pub mod ssh;
pub mod upgrade;
pub mod util;
pub mod watchdog;
pub mod watchdog_cmds;

#[allow(unused_imports)]
pub use types::*;
#[allow(unused_imports)]
pub use cli::*;
#[allow(unused_imports)]
pub use version::*;
#[allow(unused_imports)]
pub use discord::*;
#[allow(unused_imports)]
pub use channels::*;
#[allow(unused_imports)]
pub use credentials::*;
#[allow(unused_imports)]
pub use agent::*;
#[allow(unused_imports)]
pub use app_logs::*;
#[allow(unused_imports)]
pub use backup::*;
#[allow(unused_imports)]
pub use config::*;
#[allow(unused_imports)]
pub use cron::*;
#[allow(unused_imports)]
pub use discover_local::*;
#[allow(unused_imports)]
pub use discovery::*;
#[allow(unused_imports)]
pub use doctor::*;
#[allow(unused_imports)]
pub use doctor_assistant::*;
#[allow(unused_imports)]
pub use gateway::*;
#[allow(unused_imports)]
pub use instance::*;
#[allow(unused_imports)]
pub use logs::*;
#[allow(unused_imports)]
pub use model::*;
#[allow(unused_imports)]
pub use overview::*;
#[allow(unused_imports)]
pub use perf::*;
#[allow(unused_imports)]
pub use precheck::*;
#[allow(unused_imports)]
pub use preferences::*;
#[allow(unused_imports)]
pub use profiles::*;
#[allow(unused_imports)]
pub use recipe_cmds::*;
#[allow(unused_imports)]
pub use rescue::*;
#[allow(unused_imports)]
pub use sessions::*;
#[allow(unused_imports)]
pub use ssh::*;
#[allow(unused_imports)]
pub use upgrade::*;
#[allow(unused_imports)]
pub use util::*;
#[allow(unused_imports)]
pub use watchdog::*;
#[allow(unused_imports)]
pub use watchdog_cmds::*;

static REMOTE_OPENCLAW_CONFIG_PATH_CACHE: LazyLock<Mutex<HashMap<String, (String, Instant)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
const REMOTE_OPENCLAW_CONFIG_PATH_CACHE_TTL: Duration = Duration::from_secs(300);

use crate::recipe::{
    build_candidate_config_from_template, collect_change_paths, format_diff, ApplyResult,
    PreviewResult,
};

// Types are defined in types.rs and re-exported above.


/// Fast status: reads config + quick TCP probe of gateway port.
/// Local status extra: openclaw version (cached) + no duplicate detection needed locally.
fn local_health_instance() -> clawpal_core::instance::Instance {
    clawpal_core::instance::Instance {
        id: "local".to_string(),
        instance_type: clawpal_core::instance::InstanceType::Local,
        label: "Local".to_string(),
        openclaw_home: crate::cli_runner::get_active_openclaw_home_override(),
        clawpal_data_dir: crate::cli_runner::get_active_clawpal_data_override(),
        ssh_host_config: None,
    }
}

fn local_cli_cache_key(suffix: &str) -> String {
    let paths = resolve_paths();
    format!("local:{}:{}", paths.openclaw_dir.to_string_lossy(), suffix)
}

/// Check if an agent has active sessions by examining sessions/sessions.json.
/// Returns true if the file exists and is larger than 2 bytes (i.e. not just "{}").
fn agent_has_sessions(base_dir: &std::path::Path, agent_id: &str) -> bool {
    let sessions_file = base_dir
        .join("agents")
        .join(agent_id)
        .join("sessions")
        .join("sessions.json");
    match std::fs::metadata(&sessions_file) {
        Ok(m) => m.len() > 2, // "{}" is 2 bytes = empty
        Err(_) => false,
    }
}

fn truncated_json_debug(value: &Value, max_chars: usize) -> String {
    let raw = value.to_string();
    if raw.chars().count() <= max_chars {
        raw
    } else {
        let mut out = raw.chars().take(max_chars).collect::<String>();
        out.push_str("...[truncated]");
        out
    }
}

fn agent_entries_from_cli_json(json: &Value) -> Result<&Vec<Value>, String> {
    json.as_array()
        .or_else(|| json.get("agents").and_then(Value::as_array))
        .or_else(|| json.get("data").and_then(Value::as_array))
        .or_else(|| json.get("items").and_then(Value::as_array))
        .or_else(|| json.get("result").and_then(Value::as_array))
        .or_else(|| {
            json.get("data")
                .and_then(|value| value.get("agents"))
                .and_then(Value::as_array)
        })
        .or_else(|| {
            json.get("result")
                .and_then(|value| value.get("agents"))
                .and_then(Value::as_array)
        })
        .ok_or_else(|| {
            let shape = match json {
                Value::Array(array) => format!("top-level array(len={})", array.len()),
                Value::Object(map) => {
                    let mut keys = map.keys().cloned().collect::<Vec<_>>();
                    keys.sort();
                    format!("top-level object keys=[{}]", keys.join(", "))
                }
                Value::Null => "top-level null".to_string(),
                Value::Bool(_) => "top-level bool".to_string(),
                Value::Number(_) => "top-level number".to_string(),
                Value::String(_) => "top-level string".to_string(),
            };
            format!(
                "agents list output is not an array ({shape}; raw={})",
                truncated_json_debug(json, 240)
            )
        })
}

pub(crate) fn count_agent_entries_from_cli_json(json: &Value) -> Result<u32, String> {
    Ok(agent_entries_from_cli_json(json)?.len() as u32)
}

/// Parse the JSON output of `openclaw agents list --json` into Vec<AgentOverview>.
/// `online_set`: if Some, use it to determine online status; if None, check local sessions.
fn parse_agents_cli_output(
    json: &Value,
    online_set: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<AgentOverview>, String> {
    let arr = agent_entries_from_cli_json(json)?;
    let paths = if online_set.is_none() {
        Some(resolve_paths())
    } else {
        None
    };
    let mut agents = Vec::new();
    for entry in arr {
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string();
        let name = entry
            .get("identityName")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let emoji = entry
            .get("identityEmoji")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let model = entry
            .get("model")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let workspace = entry
            .get("workspace")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let online = match online_set {
            Some(set) => set.contains(&id),
            None => agent_has_sessions(paths.as_ref().unwrap().base_dir.as_path(), &id),
        };
        agents.push(AgentOverview {
            id,
            name,
            emoji,
            model,
            channels: Vec::new(),
            online,
            workspace,
        });
    }
    Ok(agents)
}

#[cfg(test)]
mod parse_agents_cli_output_tests {
    use super::{count_agent_entries_from_cli_json, parse_agents_cli_output};
    use serde_json::json;

    #[test]
    fn keeps_empty_agent_lists_empty() {
        let parsed = parse_agents_cli_output(&json!([]), None).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn counts_real_agent_entries_without_implicit_main() {
        let count = count_agent_entries_from_cli_json(&json!([])).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn accepts_wrapped_agent_arrays_from_multiple_cli_shapes() {
        for payload in [
            json!({ "agents": [{ "id": "main" }] }),
            json!({ "data": [{ "id": "main" }] }),
            json!({ "items": [{ "id": "main" }] }),
            json!({ "result": [{ "id": "main" }] }),
            json!({ "data": { "agents": [{ "id": "main" }] } }),
            json!({ "result": { "agents": [{ "id": "main" }] } }),
        ] {
            let count = count_agent_entries_from_cli_json(&payload).unwrap();
            assert_eq!(count, 1);
        }
    }

    #[test]
    fn invalid_agent_shapes_include_top_level_keys_in_error() {
        let err = count_agent_entries_from_cli_json(&json!({
            "status": "ok",
            "payload": { "entries": [] }
        }))
        .unwrap_err();
        assert!(err.contains("top-level object keys=[payload, status]"));
        assert!(err.contains("\"payload\":{\"entries\":[]}"));
    }
}

fn analyze_sessions_sync() -> Result<Vec<AgentSessionAnalysis>, String> {
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

fn delete_sessions_by_ids_sync(agent_id: &str, session_ids: &[String]) -> Result<usize, String> {
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

fn preview_session_sync(agent_id: &str, session_id: &str) -> Result<Vec<Value>, String> {
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

fn collect_model_summary(cfg: &Value) -> ModelSummary {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]


fn collect_memory_overview(base_dir: &Path) -> MemorySummary {
    let memory_root = base_dir.join("memory");
    collect_file_inventory(&memory_root, Some(80))
}

fn collect_file_inventory(path: &Path, max_files: Option<usize>) -> MemorySummary {
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

fn collect_session_overview(base_dir: &Path) -> SessionSummary {
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

struct InventorySummary {
    files: usize,
    total_bytes: u64,
}

fn collect_file_inventory_with_limit(path: &Path) -> InventorySummary {
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

fn list_session_files_detailed(base_dir: &Path) -> Result<Vec<SessionFile>, String> {
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

fn collect_session_files_in_scope(
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

fn clear_agent_and_global_sessions(
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

fn clear_directory_contents(target: &Path) -> Result<usize, String> {
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

fn model_profiles_path(paths: &crate::models::OpenClawPaths) -> std::path::PathBuf {
    paths.clawpal_dir.join("model-profiles.json")
}

fn profile_to_model_value(profile: &ModelProfile) -> String {
    let provider = profile.provider.trim();
    let model = profile.model.trim();
    if provider.is_empty() {
        return model.to_string();
    }
    if model.is_empty() {
        return format!("{provider}/");
    }
    let normalized_prefix = format!("{}/", provider.to_lowercase());
    if model.to_lowercase().starts_with(&normalized_prefix) {
        model.to_string()
    } else {
        format!("{provider}/{model}")
    }
}



fn load_model_profiles(paths: &crate::models::OpenClawPaths) -> Vec<ModelProfile> {
    let path = model_profiles_path(paths);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| r#"{"profiles":[]}"#.to_string());
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Storage {
        Wrapped {
            #[serde(default)]
            profiles: Vec<ModelProfile>,
        },
        Plain(Vec<ModelProfile>),
    }
    match serde_json::from_str::<Storage>(&text).unwrap_or(Storage::Wrapped {
        profiles: Vec::new(),
    }) {
        Storage::Wrapped { profiles } => profiles,
        Storage::Plain(profiles) => profiles,
    }
}

fn save_model_profiles(
    paths: &crate::models::OpenClawPaths,
    profiles: &[ModelProfile],
) -> Result<(), String> {
    let path = model_profiles_path(paths);
    #[derive(serde::Serialize)]
    struct Storage<'a> {
        profiles: &'a [ModelProfile],
        #[serde(rename = "version")]
        version: u8,
    }
    let payload = Storage {
        profiles,
        version: 1,
    };
    let text = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    crate::config_io::write_text(&path, &text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn sync_profile_auth_to_main_agent_with_source(
    paths: &crate::models::OpenClawPaths,
    profile: &ModelProfile,
    source_base_dir: &Path,
) -> Result<(), String> {
    let resolved_key = resolve_profile_api_key(profile, source_base_dir);
    let api_key = resolved_key.trim();
    if api_key.is_empty() {
        return Ok(());
    }

    let provider = profile.provider.trim();
    if provider.is_empty() {
        return Ok(());
    }
    let auth_ref = profile.auth_ref.trim().to_string();
    let auth_ref = if auth_ref.is_empty() {
        format!("{provider}:default")
    } else {
        auth_ref
    };

    let auth_file = paths
        .base_dir
        .join("agents")
        .join("main")
        .join("agent")
        .join("auth-profiles.json");
    if let Some(parent) = auth_file.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut root = fs::read_to_string(&auth_file)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(|| serde_json::json!({ "version": 1 }));

    if !root.is_object() {
        root = serde_json::json!({ "version": 1 });
    }
    let Some(root_obj) = root.as_object_mut() else {
        return Err("failed to prepare auth profile root object".to_string());
    };

    if !root_obj.contains_key("version") {
        root_obj.insert("version".into(), Value::from(1_u64));
    }

    let profiles_val = root_obj
        .entry("profiles".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !profiles_val.is_object() {
        *profiles_val = Value::Object(Map::new());
    }
    if let Some(profiles_map) = profiles_val.as_object_mut() {
        profiles_map.insert(
            auth_ref.clone(),
            serde_json::json!({
                "type": "api_key",
                "provider": provider,
                "key": api_key,
            }),
        );
    }

    let last_good_val = root_obj
        .entry("lastGood".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !last_good_val.is_object() {
        *last_good_val = Value::Object(Map::new());
    }
    if let Some(last_good_map) = last_good_val.as_object_mut() {
        last_good_map.insert(provider.to_string(), Value::String(auth_ref));
    }

    let serialized = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    write_text(&auth_file, &serialized)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&auth_file, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn maybe_sync_main_auth_for_model_value(
    paths: &crate::models::OpenClawPaths,
    model_value: Option<String>,
) -> Result<(), String> {
    let source_base_dir = paths.base_dir.clone();
    maybe_sync_main_auth_for_model_value_with_source(paths, model_value, &source_base_dir)
}

fn maybe_sync_main_auth_for_model_value_with_source(
    paths: &crate::models::OpenClawPaths,
    model_value: Option<String>,
    source_base_dir: &Path,
) -> Result<(), String> {
    let Some(model_value) = model_value else {
        return Ok(());
    };
    let normalized = model_value.trim().to_lowercase();
    if normalized.is_empty() {
        return Ok(());
    }
    let profiles = load_model_profiles(paths);
    for profile in &profiles {
        let profile_model = profile_to_model_value(profile);
        if profile_model.trim().to_lowercase() == normalized {
            return sync_profile_auth_to_main_agent_with_source(paths, profile, source_base_dir);
        }
    }
    Ok(())
}

fn collect_main_auth_model_candidates(cfg: &Value) -> Vec<String> {
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

fn sync_main_auth_for_config(
    paths: &crate::models::OpenClawPaths,
    cfg: &Value,
) -> Result<(), String> {
    let source_base_dir = paths.base_dir.clone();
    let mut seen = HashSet::new();
    for model in collect_main_auth_model_candidates(cfg) {
        let normalized = model.trim().to_lowercase();
        if normalized.is_empty() || !seen.insert(normalized) {
            continue;
        }
        maybe_sync_main_auth_for_model_value_with_source(paths, Some(model), &source_base_dir)?;
    }
    Ok(())
}

fn sync_main_auth_for_active_config(paths: &crate::models::OpenClawPaths) -> Result<(), String> {
    let cfg = read_openclaw_config(paths)?;
    sync_main_auth_for_config(paths, &cfg)
}

fn write_config_with_snapshot(
    paths: &crate::models::OpenClawPaths,
    current_text: &str,
    next: &Value,
    source: &str,
) -> Result<(), String> {
    let _ = add_snapshot(
        &paths.history_dir,
        &paths.metadata_path,
        Some(source.to_string()),
        source,
        true,
        current_text,
        None,
    )?;
    write_json(&paths.config_path, next)
}

fn set_nested_value(root: &mut Value, path: &str, value: Option<Value>) -> Result<(), String> {
    let path = path.trim().trim_matches('.');
    if path.is_empty() {
        return Err("invalid path".into());
    }
    let mut cur = root;
    let mut parts = path.split('.').peekable();
    while let Some(part) = parts.next() {
        let is_last = parts.peek().is_none();
        let obj = cur
            .as_object_mut()
            .ok_or_else(|| "path must point to object".to_string())?;
        if is_last {
            if let Some(v) = value {
                obj.insert(part.to_string(), v);
            } else {
                obj.remove(part);
            }
            return Ok(());
        }
        let child = obj
            .entry(part.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
        if !child.is_object() {
            *child = Value::Object(Default::default());
        }
        cur = child;
    }
    unreachable!("path should have at least one segment");
}

fn set_agent_model_value(
    root: &mut Value,
    agent_id: &str,
    model: Option<String>,
) -> Result<(), String> {
    if let Some(agents) = root.pointer_mut("/agents").and_then(Value::as_object_mut) {
        if let Some(list) = agents.get_mut("list").and_then(Value::as_array_mut) {
            for agent in list {
                if agent.get("id").and_then(Value::as_str) == Some(agent_id) {
                    if let Some(agent_obj) = agent.as_object_mut() {
                        match model {
                            Some(v) => {
                                // If existing model is an object, update "primary" inside it
                                if let Some(existing) = agent_obj.get_mut("model") {
                                    if let Some(model_obj) = existing.as_object_mut() {
                                        model_obj.insert("primary".into(), Value::String(v));
                                        return Ok(());
                                    }
                                }
                                agent_obj.insert("model".into(), Value::String(v));
                            }
                            None => {
                                agent_obj.remove("model");
                            }
                        }
                    }
                    return Ok(());
                }
            }
        }
    }
    Err(format!("agent not found: {agent_id}"))
}

fn load_model_catalog(
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

fn select_catalog_from_cache(
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
fn parse_model_catalog_from_cli_output(raw: &str) -> Option<Vec<ModelCatalogProvider>> {
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

fn extract_model_catalog_from_cli(
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

fn cache_model_catalog(
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
    fn test_select_cached_catalog_same_version() {
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
    fn test_select_cached_catalog_version_mismatch_requires_refresh() {
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

    fn profile(provider: &str, model: &str) -> ModelProfile {
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
    fn test_profile_to_model_value_keeps_provider_prefix_for_nested_model_id() {
        let p = profile("openrouter", "moonshotai/kimi-k2.5");
        assert_eq!(
            profile_to_model_value(&p),
            "openrouter/moonshotai/kimi-k2.5",
        );
    }

    #[test]
    fn test_default_base_url_supports_openai_codex_family() {
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


#[cfg(test)]
mod model_profile_upsert_tests {
    use super::*;
    use std::path::PathBuf;

    fn mk_profile(
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
            api_key: api_key.map(str::to_string),
            base_url: None,
            description: None,
            enabled: true,
        }
    }

    fn mk_paths(base_dir: PathBuf, clawpal_dir: PathBuf) -> crate::models::OpenClawPaths {
        crate::models::OpenClawPaths {
            openclaw_dir: base_dir.clone(),
            config_path: base_dir.join("openclaw.json"),
            base_dir,
            history_dir: clawpal_dir.join("history"),
            metadata_path: clawpal_dir.join("metadata.json"),
            clawpal_dir,
        }
    }

    #[test]
    fn preserve_existing_auth_fields_on_edit_when_payload_is_blank() {
        let profiles = vec![mk_profile(
            "p-1",
            "kimi-coding",
            "k2p5",
            "kimi-coding:default",
            Some("sk-old"),
        )];
        let incoming = mk_profile("p-1", "kimi-coding", "k2.5", "", None);
        let content = serde_json::json!({ "profiles": profiles, "version": 1 }).to_string();
        let (persisted, next_json) =
            clawpal_core::profile::upsert_profile_in_storage_json(&content, incoming)
                .expect("upsert");
        assert_eq!(persisted.api_key.as_deref(), Some("sk-old"));
        assert_eq!(persisted.auth_ref, "kimi-coding:default");
        let next_profiles = clawpal_core::profile::list_profiles_from_storage_json(&next_json);
        assert_eq!(next_profiles[0].model, "k2.5");
    }

    #[test]
    fn reuse_provider_credentials_for_new_profile_when_missing() {
        let donor = mk_profile(
            "p-donor",
            "openrouter",
            "model-a",
            "openrouter:default",
            Some("sk-donor"),
        );
        let incoming = mk_profile("", "openrouter", "model-b", "", None);
        let content = serde_json::json!({ "profiles": [donor], "version": 1 }).to_string();
        let (saved, _) = clawpal_core::profile::upsert_profile_in_storage_json(&content, incoming)
            .expect("upsert");
        assert_eq!(saved.auth_ref, "openrouter:default");
        assert_eq!(saved.api_key.as_deref(), Some("sk-donor"));
    }

    #[test]
    fn sync_auth_can_copy_key_from_auth_ref_source_store() {
        let tmp_root =
            std::env::temp_dir().join(format!("clawpal-auth-sync-{}", uuid::Uuid::new_v4()));
        let source_base = tmp_root.join("source-openclaw");
        let target_base = tmp_root.join("target-openclaw");
        let clawpal_dir = tmp_root.join("clawpal");
        let source_auth_file = source_base
            .join("agents")
            .join("main")
            .join("agent")
            .join("auth-profiles.json");
        let target_auth_file = target_base
            .join("agents")
            .join("main")
            .join("agent")
            .join("auth-profiles.json");

        fs::create_dir_all(source_auth_file.parent().unwrap()).expect("create source auth dir");
        let source_payload = serde_json::json!({
            "version": 1,
            "profiles": {
                "kimi-coding:default": {
                    "type": "api_key",
                    "provider": "kimi-coding",
                    "key": "sk-from-source-store"
                }
            }
        });
        write_text(
            &source_auth_file,
            &serde_json::to_string_pretty(&source_payload).expect("serialize source payload"),
        )
        .expect("write source auth");

        let paths = mk_paths(target_base, clawpal_dir);
        let profile = mk_profile("p1", "kimi-coding", "k2p5", "kimi-coding:default", None);
        sync_profile_auth_to_main_agent_with_source(&paths, &profile, &source_base)
            .expect("sync auth");

        let target_text = fs::read_to_string(target_auth_file).expect("read target auth");
        let target_json: Value = serde_json::from_str(&target_text).expect("parse target auth");
        let key = target_json
            .pointer("/profiles/kimi-coding:default/key")
            .and_then(Value::as_str);
        assert_eq!(key, Some("sk-from-source-store"));

        let _ = fs::remove_dir_all(tmp_root);
    }

    #[test]
    fn resolve_key_from_auth_store_json_supports_wrapped_and_legacy_formats() {
        let wrapped = serde_json::json!({
            "version": 1,
            "profiles": {
                "kimi-coding:default": {
                    "type": "api_key",
                    "provider": "kimi-coding",
                    "key": "sk-wrapped"
                }
            }
        });
        assert_eq!(
            resolve_key_from_auth_store_json(&wrapped, "kimi-coding:default"),
            Some("sk-wrapped".to_string())
        );

        let legacy = serde_json::json!({
            "kimi-coding": {
                "type": "api_key",
                "provider": "kimi-coding",
                "key": "sk-legacy"
            }
        });
        assert_eq!(
            resolve_key_from_auth_store_json(&legacy, "kimi-coding:default"),
            Some("sk-legacy".to_string())
        );
    }

    #[test]
    fn resolve_key_from_local_auth_store_dir_reads_auth_json_when_profiles_file_missing() {
        let tmp_root =
            std::env::temp_dir().join(format!("clawpal-auth-store-test-{}", uuid::Uuid::new_v4()));
        let agent_dir = tmp_root.join("agents").join("main").join("agent");
        fs::create_dir_all(&agent_dir).expect("create agent dir");
        let legacy_auth = serde_json::json!({
            "openai": {
                "type": "api_key",
                "provider": "openai",
                "key": "sk-openai-legacy"
            }
        });
        write_text(
            &agent_dir.join("auth.json"),
            &serde_json::to_string_pretty(&legacy_auth).expect("serialize legacy auth"),
        )
        .expect("write auth.json");

        let resolved = resolve_credential_from_local_auth_store_dir(&agent_dir, "openai:default");
        assert_eq!(
            resolved.map(|credential| credential.secret),
            Some("sk-openai-legacy".to_string())
        );
        let _ = fs::remove_dir_all(tmp_root);
    }

    #[test]
    fn resolve_profile_api_key_prefers_auth_ref_store_over_direct_api_key() {
        let tmp_root =
            std::env::temp_dir().join(format!("clawpal-auth-priority-{}", uuid::Uuid::new_v4()));
        let base_dir = tmp_root.join("openclaw");
        let auth_file = base_dir
            .join("agents")
            .join("main")
            .join("agent")
            .join("auth-profiles.json");
        fs::create_dir_all(auth_file.parent().expect("auth parent")).expect("create auth dir");
        let payload = serde_json::json!({
            "version": 1,
            "profiles": {
                "anthropic:default": {
                    "type": "token",
                    "provider": "anthropic",
                    "token": "sk-anthropic-from-store"
                }
            }
        });
        write_text(
            &auth_file,
            &serde_json::to_string_pretty(&payload).expect("serialize payload"),
        )
        .expect("write auth payload");

        let profile = mk_profile(
            "p-anthropic",
            "anthropic",
            "claude-opus-4-5",
            "anthropic:default",
            Some("sk-stale-direct"),
        );
        let resolved = resolve_profile_api_key(&profile, &base_dir);
        assert_eq!(resolved, "sk-anthropic-from-store");
        let _ = fs::remove_dir_all(tmp_root);
    }

    #[test]
    fn collect_provider_api_keys_prefers_higher_priority_source_for_same_provider() {
        let tmp_root = std::env::temp_dir().join(format!(
            "clawpal-provider-key-priority-{}",
            uuid::Uuid::new_v4()
        ));
        let base_dir = tmp_root.join("openclaw");
        let auth_file = base_dir
            .join("agents")
            .join("main")
            .join("agent")
            .join("auth-profiles.json");
        fs::create_dir_all(auth_file.parent().expect("auth parent")).expect("create auth dir");
        let payload = serde_json::json!({
            "version": 1,
            "profiles": {
                "anthropic:default": {
                    "type": "token",
                    "provider": "anthropic",
                    "token": "sk-anthropic-good"
                }
            }
        });
        write_text(
            &auth_file,
            &serde_json::to_string_pretty(&payload).expect("serialize payload"),
        )
        .expect("write auth payload");
        let stale = mk_profile(
            "anthropic-stale",
            "anthropic",
            "claude-opus-4-5",
            "",
            Some("sk-anthropic-stale"),
        );
        let preferred = mk_profile(
            "anthropic-ref",
            "anthropic",
            "claude-opus-4-6",
            "anthropic:default",
            None,
        );
        let creds = collect_provider_credentials_from_profiles(
            &[stale.clone(), preferred.clone()],
            &base_dir,
        );
        let anthropic = creds
            .get("anthropic")
            .expect("anthropic credential should exist");
        assert_eq!(anthropic.secret, "sk-anthropic-good");
        assert_eq!(anthropic.kind, InternalAuthKind::Authorization);
        let _ = fs::remove_dir_all(tmp_root);
    }

    #[test]
    fn collect_main_auth_candidates_prefers_defaults_and_main_agent() {
        let cfg = serde_json::json!({
            "agents": {
                "defaults": {
                    "model": { "primary": "kimi-coding/k2p5" }
                },
                "list": [
                    { "id": "main", "model": "anthropic/claude-opus-4-6" },
                    { "id": "worker", "model": "openai/gpt-4.1" }
                ]
            }
        });
        let models = collect_main_auth_model_candidates(&cfg);
        assert_eq!(
            models,
            vec![
                "kimi-coding/k2p5".to_string(),
                "anthropic/claude-opus-4-6".to_string(),
            ]
        );
    }

    #[test]
    fn infer_resolved_credential_kind_detects_oauth_ref() {
        let profile = mk_profile(
            "p-oauth",
            "openai-codex",
            "gpt-5",
            "openai-codex:default",
            None,
        );
        assert_eq!(
            infer_resolved_credential_kind(
                &profile,
                Some(ResolvedCredentialSource::ExplicitAuthRef)
            ),
            ResolvedCredentialKind::OAuth
        );
    }

    #[test]
    fn infer_resolved_credential_kind_detects_env_ref() {
        let profile = mk_profile("p-env", "openai", "gpt-4o", "OPENAI_API_KEY", None);
        assert_eq!(
            infer_resolved_credential_kind(
                &profile,
                Some(ResolvedCredentialSource::ExplicitAuthRef)
            ),
            ResolvedCredentialKind::EnvRef
        );
    }

    #[test]
    fn infer_resolved_credential_kind_detects_manual_and_unset() {
        let manual = mk_profile(
            "p-manual",
            "openrouter",
            "deepseek-v3",
            "",
            Some("sk-manual"),
        );
        assert_eq!(
            infer_resolved_credential_kind(&manual, Some(ResolvedCredentialSource::ManualApiKey)),
            ResolvedCredentialKind::Manual
        );
        assert_eq!(
            infer_resolved_credential_kind(&manual, None),
            ResolvedCredentialKind::Manual
        );

        let unset = mk_profile("p-unset", "openrouter", "deepseek-v3", "", None);
        assert_eq!(
            infer_resolved_credential_kind(&unset, None),
            ResolvedCredentialKind::Unset
        );
    }

    #[test]
    fn infer_resolved_credential_kind_does_not_treat_plain_openai_as_oauth() {
        let profile = mk_profile("p-openai", "openai", "gpt-4o", "openai:default", None);
        assert_eq!(
            infer_resolved_credential_kind(
                &profile,
                Some(ResolvedCredentialSource::ExplicitAuthRef)
            ),
            ResolvedCredentialKind::EnvRef
        );
    }
}

#[cfg(test)]

fn collect_agent_ids(cfg: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(agents) = cfg
        .get("agents")
        .and_then(|v| v.get("list"))
        .and_then(Value::as_array)
    {
        for agent in agents {
            if let Some(id) = agent.get("id").and_then(Value::as_str) {
                ids.push(id.to_string());
            }
        }
    }
    // Implicit "main" agent when no agents.list
    if ids.is_empty() {
        ids.push("main".into());
    }
    ids
}

fn collect_model_bindings(cfg: &Value, profiles: &[ModelProfile]) -> Vec<ModelBinding> {
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

    fn walk_channel_binding(
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

fn find_profile_by_model(profiles: &[ModelProfile], value: Option<&str>) -> Option<String> {
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

fn resolve_auth_ref_for_provider(cfg: &Value, provider: &str) -> Option<String> {
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

// resolve_full_api_key is intentionally not exposed as a Tauri command.
// It returns raw API keys which should never be sent to the frontend.
#[allow(dead_code)]
fn resolve_full_api_key(profile_id: String) -> Result<String, String> {
    let paths = resolve_paths();
    let profiles = load_model_profiles(&paths);
    let profile = profiles
        .iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| "Profile not found".to_string())?;
    let key = resolve_profile_api_key(profile, &paths.base_dir);
    if key.is_empty() {
        return Err("No API key configured for this profile".to_string());
    }
    Ok(key)
}

// ---- Backup / Restore ----


fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    skip_dirs: &HashSet<&str>,
    total: &mut u64,
) -> Result<(), String> {
    let entries =
        fs::read_dir(src).map_err(|e| format!("Failed to read dir {}: {e}", src.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip the config file (already copied separately) and skip dirs
        if name_str == "openclaw.json" {
            continue;
        }

        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let dest = dst.join(&name);

        if file_type.is_dir() {
            if skip_dirs.contains(name_str.as_ref()) {
                continue;
            }
            fs::create_dir_all(&dest)
                .map_err(|e| format!("Failed to create dir {}: {e}", dest.display()))?;
            copy_dir_recursive(&entry.path(), &dest, skip_dirs, total)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &dest)
                .map_err(|e| format!("Failed to copy {}: {e}", name_str))?;
            *total += fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        }
    }
    Ok(())
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                total += dir_size(&entry.path());
            } else {
                total += fs::metadata(entry.path()).map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

fn restore_dir_recursive(src: &Path, dst: &Path, skip_dirs: &HashSet<&str>) -> Result<(), String> {
    let entries = fs::read_dir(src).map_err(|e| format!("Failed to read backup dir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str == "openclaw.json" {
            continue; // Already restored separately
        }

        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let dest = dst.join(&name);

        if file_type.is_dir() {
            if skip_dirs.contains(name_str.as_ref()) {
                continue;
            }
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            restore_dir_recursive(&entry.path(), &dest, skip_dirs)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &dest)
                .map_err(|e| format!("Failed to restore {}: {e}", name_str))?;
        }
    }
    Ok(())
}

// ---- Remote Backup / Restore (via SSH) ----

fn resolve_model_provider_base_url(cfg: &Value, provider: &str) -> Option<String> {
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

// ---------------------------------------------------------------------------
// Task 6: Remote business commands
// ---------------------------------------------------------------------------

fn is_owner_display_parse_error(text: &str) -> bool {
    clawpal_core::doctor::owner_display_parse_error(text)
}

async fn run_openclaw_remote_with_autofix(
    pool: &SshConnectionPool,
    host_id: &str,
    args: &[&str],
) -> Result<crate::cli_runner::CliOutput, String> {
    let first = crate::cli_runner::run_openclaw_remote(pool, host_id, args).await?;
    if first.exit_code == 0 {
        return Ok(first);
    }
    let combined = format!("{}\n{}", first.stderr, first.stdout);
    if !is_owner_display_parse_error(&combined) {
        return Ok(first);
    }
    let _ = crate::cli_runner::run_openclaw_remote(pool, host_id, &["doctor", "--fix"]).await;
    crate::cli_runner::run_openclaw_remote(pool, host_id, args).await
}

/// Tier 2: slow, optional — openclaw version + duplicate detection (2 SSH calls in parallel).
/// Called once on mount and on-demand (e.g., after upgrade), not in poll loop.
// ---------------------------------------------------------------------------
// Remote config mutation helpers & commands
// ---------------------------------------------------------------------------

/// Private helper: snapshot current config then write new config on remote.
async fn remote_write_config_with_snapshot(
    pool: &SshConnectionPool,
    host_id: &str,
    config_path: &str,
    current_text: &str,
    next: &Value,
    source: &str,
) -> Result<(), String> {
    // Use core function to prepare config write
    let (new_text, snapshot_text) =
        clawpal_core::config::prepare_config_write(current_text, next, source)?;

    // Create snapshot dir
    pool.exec(host_id, "mkdir -p ~/.clawpal/snapshots").await?;

    // Generate snapshot filename
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let snapshot_path = clawpal_core::config::snapshot_filename(ts, source);
    let snapshot_full_path = format!("~/.clawpal/snapshots/{snapshot_path}");

    // Write snapshot and new config via SFTP
    pool.sftp_write(host_id, &snapshot_full_path, &snapshot_text)
        .await?;
    pool.sftp_write(host_id, config_path, &new_text).await?;
    Ok(())
}

async fn remote_resolve_openclaw_config_path(
    pool: &SshConnectionPool,
    host_id: &str,
) -> Result<String, String> {
    if let Ok(cache) = REMOTE_OPENCLAW_CONFIG_PATH_CACHE.lock() {
        if let Some((path, cached_at)) = cache.get(host_id) {
            if cached_at.elapsed() < REMOTE_OPENCLAW_CONFIG_PATH_CACHE_TTL {
                return Ok(path.clone());
            }
        }
    }
    let result = pool
        .exec_login(
            host_id,
            clawpal_core::doctor::remote_openclaw_config_path_probe_script(),
        )
        .await?;
    if result.exit_code != 0 {
        let details = format!("{}\n{}", result.stderr.trim(), result.stdout.trim());
        return Err(format!(
            "Failed to resolve remote openclaw config path ({}): {}",
            result.exit_code,
            details.trim()
        ));
    }
    let path = result.stdout.trim();
    if path.is_empty() {
        return Err("Remote openclaw config path probe returned empty output".into());
    }
    if let Ok(mut cache) = REMOTE_OPENCLAW_CONFIG_PATH_CACHE.lock() {
        cache.insert(host_id.to_string(), (path.to_string(), Instant::now()));
    }
    Ok(path.to_string())
}

async fn remote_read_openclaw_config_text_and_json(
    pool: &SshConnectionPool,
    host_id: &str,
) -> Result<(String, String, Value), String> {
    let config_path = remote_resolve_openclaw_config_path(pool, host_id).await?;
    let raw = pool.sftp_read(host_id, &config_path).await?;
    let (parsed, normalized) = clawpal_core::config::parse_and_normalize_config(&raw)
        .map_err(|e| format!("Failed to parse remote config: {e}"))?;
    Ok((config_path, normalized, parsed))
}

async fn run_remote_rescue_bot_command(
    pool: &SshConnectionPool,
    host_id: &str,
    command: Vec<String>,
) -> Result<RescueBotCommandResult, String> {
    let output = run_remote_openclaw_raw(pool, host_id, &command).await?;
    if is_gateway_status_command_output_incompatible(&output, &command) {
        let fallback_command = strip_gateway_status_json_flag(&command);
        if fallback_command != command {
            let fallback_output = run_remote_openclaw_raw(pool, host_id, &fallback_command).await?;
            return Ok(RescueBotCommandResult {
                command: fallback_command,
                output: fallback_output,
            });
        }
    }
    Ok(RescueBotCommandResult { command, output })
}

async fn run_remote_openclaw_raw(
    pool: &SshConnectionPool,
    host_id: &str,
    command: &[String],
) -> Result<OpenclawCommandOutput, String> {
    let args = command.iter().map(String::as_str).collect::<Vec<_>>();
    let raw = crate::cli_runner::run_openclaw_remote(pool, host_id, &args).await?;
    Ok(OpenclawCommandOutput {
        stdout: raw.stdout,
        stderr: raw.stderr,
        exit_code: raw.exit_code,
    })
}

async fn run_remote_openclaw_dynamic(
    pool: &SshConnectionPool,
    host_id: &str,
    command: Vec<String>,
) -> Result<OpenclawCommandOutput, String> {
    Ok(run_remote_rescue_bot_command(pool, host_id, command)
        .await?
        .output)
}

async fn run_remote_primary_doctor_with_fallback(
    pool: &SshConnectionPool,
    host_id: &str,
    profile: &str,
) -> Result<OpenclawCommandOutput, String> {
    let json_command = build_profile_command(profile, &["doctor", "--json", "--yes"]);
    let output = run_remote_openclaw_dynamic(pool, host_id, json_command).await?;
    if output.exit_code != 0
        && clawpal_core::doctor::doctor_json_option_unsupported(&output.stderr, &output.stdout)
    {
        let plain_command = build_profile_command(profile, &["doctor", "--yes"]);
        return run_remote_openclaw_dynamic(pool, host_id, plain_command).await;
    }
    Ok(output)
}

async fn run_remote_gateway_restart_fallback(
    pool: &SshConnectionPool,
    host_id: &str,
    profile: &str,
    commands: &mut Vec<RescueBotCommandResult>,
) -> Result<(), String> {
    let stop_command = vec![
        "--profile".to_string(),
        profile.to_string(),
        "gateway".to_string(),
        "stop".to_string(),
    ];
    let stop_result = run_remote_rescue_bot_command(pool, host_id, stop_command).await?;
    commands.push(stop_result);

    let start_command = vec![
        "--profile".to_string(),
        profile.to_string(),
        "gateway".to_string(),
        "start".to_string(),
    ];
    let start_result = run_remote_rescue_bot_command(pool, host_id, start_command).await?;
    if start_result.output.exit_code != 0 {
        return Err(command_failure_message(
            &start_result.command,
            &start_result.output,
        ));
    }
    commands.push(start_result);
    Ok(())
}

fn is_remote_missing_path_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("no such file")
        || lower.contains("no such file or directory")
        || lower.contains("not found")
        || lower.contains("cannot open")
}


async fn read_remote_env_var(
    pool: &SshConnectionPool,
    host_id: &str,
    name: &str,
) -> Result<Option<String>, String> {
    if !is_valid_env_var_name(name) {
        return Err(format!("Invalid environment variable name: {name}"));
    }

    let cmd = format!("printenv -- {name}");
    let out = pool
        .exec_login(host_id, &cmd)
        .await
        .map_err(|e| format!("Failed to read remote env var {name}: {e}"))?;

    if out.exit_code != 0 {
        return Ok(None);
    }

    let value = out.stdout.trim();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value.to_string()))
    }
}

async fn resolve_remote_key_from_agent_auth_profiles(
    pool: &SshConnectionPool,
    host_id: &str,
    auth_ref: &str,
) -> Result<Option<String>, String> {
    let roots = resolve_remote_openclaw_roots(pool, host_id).await?;

    for root in roots {
        let agents_path = format!("{}/agents", root.trim_end_matches('/'));
        let entries = match pool.sftp_list(host_id, &agents_path).await {
            Ok(entries) => entries,
            Err(e) if is_remote_missing_path_error(&e) => continue,
            Err(e) => {
                return Err(format!(
                    "Failed to list remote agents directory at {agents_path}: {e}"
                ))
            }
        };

        for agent in entries.into_iter().filter(|entry| entry.is_dir) {
            let agent_dir = format!("{}/agents/{}/agent", root.trim_end_matches('/'), agent.name);
            for file_name in ["auth-profiles.json", "auth.json"] {
                let auth_file = format!("{agent_dir}/{file_name}");
                let text = match pool.sftp_read(host_id, &auth_file).await {
                    Ok(text) => text,
                    Err(e) if is_remote_missing_path_error(&e) => continue,
                    Err(e) => {
                        return Err(format!(
                            "Failed to read remote auth store at {auth_file}: {e}"
                        ))
                    }
                };
                let data: Value = serde_json::from_str(&text).map_err(|e| {
                    format!("Failed to parse remote auth store at {auth_file}: {e}")
                })?;
                // Try plaintext first, then resolve SecretRef env vars from remote.
                if let Some(key) = resolve_key_from_auth_store_json(&data, auth_ref) {
                    return Ok(Some(key));
                }
                // Collect env-source SecretRef names and fetch them from remote host.
                let sr_env_names = collect_secret_ref_env_names_from_auth_store(&data);
                if !sr_env_names.is_empty() {
                    let remote_env =
                        RemoteAuthCache::batch_read_env_vars(pool, host_id, &sr_env_names)
                            .await
                            .unwrap_or_default();
                    let env_lookup =
                        |name: &str| -> Option<String> { remote_env.get(name).cloned() };
                    if let Some(key) =
                        resolve_key_from_auth_store_json_with_env(&data, auth_ref, &env_lookup)
                    {
                        return Ok(Some(key));
                    }
                }
            }
        }
    }

    Ok(None)
}

async fn resolve_remote_openclaw_roots(
    pool: &SshConnectionPool,
    host_id: &str,
) -> Result<Vec<String>, String> {
    let mut roots = Vec::<String>::new();
    let primary = pool
        .exec_login(
            host_id,
            clawpal_core::doctor::remote_openclaw_root_probe_script(),
        )
        .await?;
    let primary_trimmed = primary.stdout.trim();
    if !primary_trimmed.is_empty() {
        roots.push(primary_trimmed.to_string());
    }

    let discover = pool
        .exec_login(
            host_id,
            "for d in \"$HOME\"/.openclaw*; do [ -d \"$d\" ] && printf '%s\\n' \"$d\"; done",
        )
        .await?;
    for line in discover.stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            roots.push(trimmed.to_string());
        }
    }
    let mut deduped = Vec::<String>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();
    for root in roots {
        if seen.insert(root.clone()) {
            deduped.push(root);
        }
    }
    roots = deduped;
    Ok(roots)
}

async fn resolve_remote_profile_base_url(
    pool: &SshConnectionPool,
    host_id: &str,
    profile: &ModelProfile,
) -> Result<Option<String>, String> {
    if let Some(base) = profile
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return Ok(Some(base.to_string()));
    }

    let config_path = match remote_resolve_openclaw_config_path(pool, host_id).await {
        Ok(path) => path,
        Err(_) => return Ok(None),
    };
    let raw = match pool.sftp_read(host_id, &config_path).await {
        Ok(raw) => raw,
        Err(e) if is_remote_missing_path_error(&e) => return Ok(None),
        Err(e) => {
            return Err(format!(
                "Failed to read remote config for base URL resolution: {e}"
            ))
        }
    };
    let cfg = match clawpal_core::config::parse_and_normalize_config(&raw) {
        Ok((parsed, _)) => parsed,
        Err(e) => {
            return Err(format!(
                "Failed to parse remote config for base URL resolution: {e}"
            ))
        }
    };
    Ok(resolve_model_provider_base_url(&cfg, &profile.provider))
}

async fn resolve_remote_profile_api_key(
    pool: &SshConnectionPool,
    host_id: &str,
    profile: &ModelProfile,
) -> Result<String, String> {
    let auth_ref = profile.auth_ref.trim();
    let has_explicit_auth_ref = !auth_ref.is_empty();

    // 1. Explicit auth_ref (user-specified): env var, then auth store.
    if has_explicit_auth_ref {
        if is_valid_env_var_name(auth_ref) {
            if let Some(key) = read_remote_env_var(pool, host_id, auth_ref).await? {
                return Ok(key);
            }
        }
        if let Some(key) =
            resolve_remote_key_from_agent_auth_profiles(pool, host_id, auth_ref).await?
        {
            return Ok(key);
        }
    }

    // 2. Direct api_key before fallback auth refs/env conventions.
    if let Some(key) = &profile.api_key {
        let trimmed_key = key.trim();
        if !trimmed_key.is_empty() {
            return Ok(trimmed_key.to_string());
        }
    }

    // 3. Fallback provider:default auth_ref from auth store.
    let provider = profile.provider.trim().to_lowercase();
    if !provider.is_empty() {
        let fallback = format!("{provider}:default");
        let skip = has_explicit_auth_ref && auth_ref == fallback;
        if !skip {
            if let Some(key) =
                resolve_remote_key_from_agent_auth_profiles(pool, host_id, &fallback).await?
            {
                return Ok(key);
            }
        }
    }

    // 4. Provider env var conventions.
    for env_name in provider_env_var_candidates(&profile.provider) {
        if let Some(key) = read_remote_env_var(pool, host_id, &env_name).await? {
            return Ok(key);
        }
    }

    Ok(String::new())
}

// ---------------------------------------------------------------------------
// Batched remote auth resolution — pre-fetches env vars and auth store files
// in bulk (2-3 SSH calls total) instead of 5-7 per profile.
// ---------------------------------------------------------------------------

struct RemoteAuthCache {
    env_vars: HashMap<String, String>,
    auth_store_files: Vec<Value>,
}

impl RemoteAuthCache {
    /// Build cache by collecting all needed env var names from all profiles
    /// (including SecretRef env vars from auth stores) and reading them +
    /// all auth-store files in bulk.
    async fn build(
        pool: &SshConnectionPool,
        host_id: &str,
        profiles: &[ModelProfile],
    ) -> Result<Self, String> {
        // Collect env var names needed from profile auth_refs and provider conventions.
        let mut env_var_names = Vec::<String>::new();
        let mut seen_env = std::collections::HashSet::<String>::new();
        for profile in profiles {
            let auth_ref = profile.auth_ref.trim();
            if !auth_ref.is_empty()
                && is_valid_env_var_name(auth_ref)
                && seen_env.insert(auth_ref.to_string())
            {
                env_var_names.push(auth_ref.to_string());
            }
            for env_name in provider_env_var_candidates(&profile.provider) {
                if seen_env.insert(env_name.clone()) {
                    env_var_names.push(env_name);
                }
            }
        }

        // Read all auth-store files from remote agents first so we can
        // discover additional env var names referenced by SecretRefs.
        let auth_store_files = Self::read_auth_store_files(pool, host_id).await?;

        // Scan auth store files for env-source SecretRef references and
        // include their env var names in the batch read.
        for data in &auth_store_files {
            for name in collect_secret_ref_env_names_from_auth_store(data) {
                if seen_env.insert(name.clone()) {
                    env_var_names.push(name);
                }
            }
        }

        // Batch-read all env vars in a single SSH call.
        let env_vars = if env_var_names.is_empty() {
            HashMap::new()
        } else {
            Self::batch_read_env_vars(pool, host_id, &env_var_names).await?
        };

        Ok(Self {
            env_vars,
            auth_store_files,
        })
    }

    async fn batch_read_env_vars(
        pool: &SshConnectionPool,
        host_id: &str,
        names: &[String],
    ) -> Result<HashMap<String, String>, String> {
        // Build a shell script that prints "NAME=VALUE\0" for each set var.
        // Using NUL delimiter avoids issues with newlines in values.
        let mut script = String::from("for __v in");
        for name in names {
            // All names are validated by is_valid_env_var_name, safe to interpolate.
            script.push(' ');
            script.push_str(name);
        }
        script.push_str("; do eval \"__val=\\${$__v+__SET__}\\${$__v}\"; ");
        script.push_str("case \"$__val\" in __SET__*) printf '%s=%s\\n' \"$__v\" \"${__val#__SET__}\";; esac; done");

        let out = pool
            .exec_login(host_id, &script)
            .await
            .map_err(|e| format!("Failed to batch-read remote env vars: {e}"))?;

        let mut map = HashMap::new();
        for line in out.stdout.lines() {
            if let Some(eq_pos) = line.find('=') {
                let key = &line[..eq_pos];
                let val = line[eq_pos + 1..].trim();
                if !val.is_empty() {
                    map.insert(key.to_string(), val.to_string());
                }
            }
        }
        Ok(map)
    }

    async fn read_auth_store_files(
        pool: &SshConnectionPool,
        host_id: &str,
    ) -> Result<Vec<Value>, String> {
        let roots = resolve_remote_openclaw_roots(pool, host_id).await?;
        let mut store_files = Vec::new();

        for root in &roots {
            let agents_path = format!("{}/agents", root.trim_end_matches('/'));
            let entries = match pool.sftp_list(host_id, &agents_path).await {
                Ok(entries) => entries,
                Err(e) if is_remote_missing_path_error(&e) => continue,
                Err(_) => continue,
            };

            for agent in entries.into_iter().filter(|entry| entry.is_dir) {
                let agent_dir =
                    format!("{}/agents/{}/agent", root.trim_end_matches('/'), agent.name);
                for file_name in ["auth-profiles.json", "auth.json"] {
                    let auth_file = format!("{agent_dir}/{file_name}");
                    let text = match pool.sftp_read(host_id, &auth_file).await {
                        Ok(text) => text,
                        Err(_) => continue,
                    };
                    if let Ok(data) = serde_json::from_str::<Value>(&text) {
                        store_files.push(data);
                    }
                }
            }
        }
        Ok(store_files)
    }

    /// Resolve API key for a single profile using cached data.
    fn resolve_for_profile_with_source(
        &self,
        profile: &ModelProfile,
    ) -> Option<(String, ResolvedCredentialSource)> {
        let auth_ref = profile.auth_ref.trim();
        let has_explicit_auth_ref = !auth_ref.is_empty();

        // 1. Explicit auth_ref as env var, then auth store.
        if has_explicit_auth_ref {
            if is_valid_env_var_name(auth_ref) {
                if let Some(val) = self.env_vars.get(auth_ref) {
                    return Some((val.clone(), ResolvedCredentialSource::ExplicitAuthRef));
                }
            }
            if let Some(key) = self.find_in_auth_stores(auth_ref) {
                return Some((key, ResolvedCredentialSource::ExplicitAuthRef));
            }
        }

        // 2. Direct api_key — before fallback auth_ref.
        if let Some(ref key) = profile.api_key {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                return Some((trimmed.to_string(), ResolvedCredentialSource::ManualApiKey));
            }
        }

        // 3. Fallback provider:default auth_ref.
        let provider = profile.provider.trim().to_lowercase();
        if !provider.is_empty() {
            let fallback = format!("{provider}:default");
            let skip = has_explicit_auth_ref && auth_ref == fallback;
            if !skip {
                if let Some(key) = self.find_in_auth_stores(&fallback) {
                    return Some((key, ResolvedCredentialSource::ProviderFallbackAuthRef));
                }
            }
        }

        // 4. Provider env var conventions.
        for env_name in provider_env_var_candidates(&profile.provider) {
            if let Some(val) = self.env_vars.get(&env_name) {
                return Some((val.clone(), ResolvedCredentialSource::ProviderEnvVar));
            }
        }

        None
    }

    fn resolve_for_profile(&self, profile: &ModelProfile) -> String {
        self.resolve_for_profile_with_source(profile)
            .map(|(key, _)| key)
            .unwrap_or_default()
    }

    fn find_in_auth_stores(&self, auth_ref: &str) -> Option<String> {
        let env_lookup = |name: &str| -> Option<String> { self.env_vars.get(name).cloned() };
        for data in &self.auth_store_files {
            if let Some(key) =
                resolve_key_from_auth_store_json_with_env(data, auth_ref, &env_lookup)
            {
                return Some(key);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Cron jobs
// ---------------------------------------------------------------------------

fn parse_cron_jobs(text: &str) -> Value {
    let jobs = clawpal_core::cron::parse_cron_jobs(text).unwrap_or_default();
    Value::Array(jobs)
}

// ---------------------------------------------------------------------------
// Remote cron jobs
// ---------------------------------------------------------------------------
