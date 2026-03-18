/// Macro for wrapping synchronous command bodies with timing.
/// Uses a closure to capture `?` early-returns so timing is always recorded.
macro_rules! timed_sync {
    ($name:expr, $body:block) => {{
        let __start = std::time::Instant::now();
        let __result = (|| $body)();
        let __elapsed_us = __start.elapsed().as_micros() as u64;
        crate::commands::perf::record_timing($name, __elapsed_us);
        __result
    }};
}

/// Macro for wrapping async command bodies with timing.
/// Uses an async block to capture `?` early-returns so timing is always recorded.
macro_rules! timed_async {
    ($name:expr, $body:block) => {{
        let __start = std::time::Instant::now();
        let __result = async $body.await;
        let __elapsed_us = __start.elapsed().as_micros() as u64;
        crate::commands::perf::record_timing($name, __elapsed_us);
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

pub mod channels;
pub mod cli;
pub mod credentials;
pub mod discord;
pub mod types;
pub mod version;

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
pub use agent::*;
#[allow(unused_imports)]
pub use app_logs::*;
#[allow(unused_imports)]
pub use backup::*;
#[allow(unused_imports)]
pub use channels::*;
#[allow(unused_imports)]
pub use cli::*;
#[allow(unused_imports)]
pub use config::*;
#[allow(unused_imports)]
pub use credentials::*;
#[allow(unused_imports)]
pub use cron::*;
#[allow(unused_imports)]
pub use discord::*;
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
pub use types::*;
#[allow(unused_imports)]
pub use upgrade::*;
#[allow(unused_imports)]
pub use util::*;
#[allow(unused_imports)]
pub use version::*;
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

pub(crate) fn count_agent_entries_from_cli_json(json: &Value) -> Result<u32, String> {
    Ok(agent_entries_from_cli_json(json)?.len() as u32)
}

fn read_model_value(value: &Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        return Some(value.to_string());
    }

    if let Some(model_obj) = value.as_object() {
        if let Some(primary) = model_obj.get("primary").and_then(Value::as_str) {
            return Some(primary.to_string());
        }
        if let Some(name) = model_obj.get("name").and_then(Value::as_str) {
            return Some(name.to_string());
        }
        if let Some(model) = model_obj.get("model").and_then(Value::as_str) {
            return Some(model.to_string());
        }
        if let Some(model) = model_obj.get("default").and_then(Value::as_str) {
            return Some(model.to_string());
        }
        if let Some(v) = model_obj.get("provider").and_then(Value::as_str) {
            if let Some(inner) = model_obj.get("id").and_then(Value::as_str) {
                return Some(format!("{v}/{inner}"));
            }
        }
    }
    None
}

fn collect_memory_overview(base_dir: &Path) -> MemorySummary {
    let memory_root = base_dir.join("memory");
    collect_file_inventory(&memory_root, Some(80))
}
