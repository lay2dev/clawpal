use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceTokenRecord {
    pub token: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

pub trait AuthStore {
    fn load(&self, device_id: &str, role: &str) -> Result<Option<DeviceTokenRecord>, Error>;
    fn store(&self, device_id: &str, role: &str, record: &DeviceTokenRecord) -> Result<(), Error>;
    fn clear(&self, device_id: &str, role: &str) -> Result<(), Error>;
}

#[derive(Debug, Clone)]
pub struct FileAuthStore {
    root: PathBuf,
}

impl FileAuthStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn path_for(&self, device_id: &str, role: &str) -> PathBuf {
        self.root.join(sanitize(device_id)).join(format!("{}.json", sanitize(role)))
    }
}

impl AuthStore for FileAuthStore {
    fn load(&self, device_id: &str, role: &str) -> Result<Option<DeviceTokenRecord>, Error> {
        let path = self.path_for(device_id, role);
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)?;
        Ok(Some(serde_json::from_str(&raw)?))
    }

    fn store(&self, device_id: &str, role: &str, record: &DeviceTokenRecord) -> Result<(), Error> {
        let path = self.path_for(device_id, role);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(record)?;
        fs::write(path, raw)?;
        Ok(())
    }

    fn clear(&self, device_id: &str, role: &str) -> Result<(), Error> {
        let path = self.path_for(device_id, role);
        if path.exists() {
            fs::remove_file(&path)?;
            prune_empty_dirs(&self.root, path.parent());
        }
        Ok(())
    }
}

fn sanitize(value: &str) -> String {
    value.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn prune_empty_dirs(root: &Path, current: Option<&Path>) {
    let mut current = current;
    while let Some(path) = current {
        if path == root {
            break;
        }
        let is_empty = fs::read_dir(path)
            .ok()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if !is_empty {
            break;
        }
        let parent = path.parent();
        let _ = fs::remove_dir(path);
        current = parent;
    }
}
