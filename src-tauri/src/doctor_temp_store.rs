/// Persistent store for temporary gateway session records used by doctor assistant.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoctorTempGatewaySessionRecord {
    pub instance_id: String,
    pub profile: String,
    pub port: u16,
    pub created_at: String,
    pub status: String,
    pub main_profile: String,
    pub main_port: u16,
    pub last_step: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoctorTempGatewaySessionStore {
    pub sessions: Vec<DoctorTempGatewaySessionRecord>,
}

pub(crate) fn store_path(paths: &crate::models::OpenClawPaths) -> std::path::PathBuf {
    paths.clawpal_dir.join("doctor-temp-gateways.json")
}

pub(crate) fn load(paths: &crate::models::OpenClawPaths) -> DoctorTempGatewaySessionStore {
    crate::config_io::read_json(&store_path(paths)).unwrap_or_default()
}

pub(crate) fn save(
    paths: &crate::models::OpenClawPaths,
    store: &DoctorTempGatewaySessionStore,
) -> Result<(), String> {
    let path = store_path(paths);
    if store.sessions.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    } else {
        crate::config_io::write_json(&path, store)
    }
}

pub(crate) fn upsert(
    paths: &crate::models::OpenClawPaths,
    record: DoctorTempGatewaySessionRecord,
) -> Result<(), String> {
    let mut store = load(paths);
    store
        .sessions
        .retain(|item| !(item.instance_id == record.instance_id && item.profile == record.profile));
    store.sessions.push(record);
    save(paths, &store)
}

pub(crate) fn remove_record(
    paths: &crate::models::OpenClawPaths,
    instance_id: &str,
    profile: &str,
) -> Result<(), String> {
    let mut store = load(paths);
    store
        .sessions
        .retain(|item| !(item.instance_id == instance_id && item.profile == profile));
    save(paths, &store)
}

pub(crate) fn remove_for_instance(
    paths: &crate::models::OpenClawPaths,
    instance_id: &str,
) -> Result<(), String> {
    let mut store = load(paths);
    store
        .sessions
        .retain(|item| item.instance_id != instance_id);
    save(paths, &store)
}
