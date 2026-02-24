use super::session_store::InstallSessionStore;
use super::types::{InstallMethod, InstallSession, InstallState};
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;
use tauri::State;
use uuid::Uuid;

static TEST_SESSION_STORE: LazyLock<InstallSessionStore> = LazyLock::new(InstallSessionStore::new);

fn parse_method(raw: &str) -> Result<InstallMethod, String> {
    match raw {
        "local" => Ok(InstallMethod::Local),
        "wsl2" => Ok(InstallMethod::Wsl2),
        "docker" => Ok(InstallMethod::Docker),
        "remote_ssh" => Ok(InstallMethod::RemoteSsh),
        _ => Err(format!("unsupported install method: {raw}")),
    }
}

fn create_session(store: &InstallSessionStore, method_raw: &str) -> Result<InstallSession, String> {
    let method = parse_method(method_raw)?;
    let now = Utc::now().to_rfc3339();
    let session = InstallSession {
        id: format!("install-{}", Uuid::new_v4()),
        method,
        state: InstallState::SelectedMethod,
        current_step: None,
        logs: vec![],
        artifacts: HashMap::<String, Value>::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    store.insert(session.clone())?;
    Ok(session)
}

#[tauri::command]
pub async fn install_create_session(
    method: String,
    store: State<'_, InstallSessionStore>,
) -> Result<InstallSession, String> {
    create_session(&store, method.trim())
}

#[tauri::command]
pub async fn install_get_session(
    session_id: String,
    store: State<'_, InstallSessionStore>,
) -> Result<InstallSession, String> {
    let id = session_id.trim();
    if id.is_empty() {
        return Err("session_id is required".to_string());
    }
    match store.get(id)? {
        Some(session) => Ok(session),
        None => Err(format!("install session not found: {id}")),
    }
}

pub async fn create_session_for_test(method: &str) -> Result<InstallSession, String> {
    create_session(&TEST_SESSION_STORE, method)
}
