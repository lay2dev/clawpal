use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshHostConfig {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    pub key_path: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstanceType {
    Local,
    Docker,
    RemoteSsh,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub instance_type: InstanceType,
    pub label: String,
    pub openclaw_home: Option<String>,
    pub clawpal_data_dir: Option<String>,
    pub ssh_host_config: Option<SshHostConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RegistryFile {
    pub instances: Vec<Instance>,
}

#[derive(Debug, Clone, Default)]
pub struct InstanceRegistry {
    instances: BTreeMap<String, Instance>,
}

#[derive(Debug, Error)]
pub enum InstanceRegistryError {
    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    ParseFile {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize instances.json: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to write {path}: {source}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("instance '{0}' already exists")]
    DuplicateInstance(String),
}

pub type Result<T> = std::result::Result<T, InstanceRegistryError>;

impl InstanceRegistry {
    pub fn load() -> Result<Self> {
        let path = registry_path();
        if !path.exists() {
            return Ok(Self::default());
        }

        let data = fs::read_to_string(&path).map_err(|source| InstanceRegistryError::ReadFile {
            path: path.clone(),
            source,
        })?;
        let parsed: RegistryFile = serde_json::from_str(&data)
            .map_err(|source| InstanceRegistryError::ParseFile { path, source })?;

        let instances = parsed
            .instances
            .into_iter()
            .map(|instance| (instance.id.clone(), instance))
            .collect();
        Ok(Self { instances })
    }

    pub fn save(&self) -> Result<()> {
        let path = registry_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| InstanceRegistryError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let body = RegistryFile {
            instances: self.list(),
        };
        let json = serde_json::to_string_pretty(&body)?;
        fs::write(&path, json)
            .map_err(|source| InstanceRegistryError::WriteFile { path, source })?;
        Ok(())
    }

    pub fn list(&self) -> Vec<Instance> {
        self.instances.values().cloned().collect()
    }

    pub fn add(&mut self, instance: Instance) -> Result<()> {
        if self.instances.contains_key(&instance.id) {
            return Err(InstanceRegistryError::DuplicateInstance(instance.id));
        }
        self.instances.insert(instance.id.clone(), instance);
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> Option<Instance> {
        self.instances.remove(id)
    }

    pub fn get(&self, id: &str) -> Option<&Instance> {
        self.instances.get(id)
    }
}

fn registry_path() -> PathBuf {
    if let Ok(dir) = std::env::var("CLAWPAL_DATA_DIR") {
        return PathBuf::from(dir).join("instances.json");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".clawpal").join("instances.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_data_dir() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("clawpal-core-instance-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn sample_instance(id: &str) -> Instance {
        Instance {
            id: id.to_string(),
            instance_type: InstanceType::Docker,
            label: "Docker Local".to_string(),
            openclaw_home: Some("/tmp/openclaw".to_string()),
            clawpal_data_dir: Some("/tmp/clawpal".to_string()),
            ssh_host_config: None,
        }
    }

    #[test]
    fn load_returns_empty_when_file_missing() {
        let _guard = crate::test_support::env_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_data_dir();
        std::env::set_var("CLAWPAL_DATA_DIR", &dir);

        let registry = InstanceRegistry::load().expect("load registry");
        assert!(registry.list().is_empty());
    }

    #[test]
    fn save_persists_instances_to_disk() {
        let _guard = crate::test_support::env_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_data_dir();
        std::env::set_var("CLAWPAL_DATA_DIR", &dir);

        let mut registry = InstanceRegistry::default();
        registry.add(sample_instance("docker:local")).expect("add");
        registry.save().expect("save");

        let path = dir.join("instances.json");
        assert!(path.exists());
    }

    #[test]
    fn list_returns_registered_instances() {
        let mut registry = InstanceRegistry::default();
        registry.add(sample_instance("docker:a")).expect("add");
        registry.add(sample_instance("docker:b")).expect("add");

        let list = registry.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn add_rejects_duplicate_id() {
        let mut registry = InstanceRegistry::default();
        registry
            .add(sample_instance("docker:dup"))
            .expect("first add");
        let err = registry
            .add(sample_instance("docker:dup"))
            .expect_err("duplicate should fail");
        assert!(matches!(err, InstanceRegistryError::DuplicateInstance(_)));
    }

    #[test]
    fn remove_deletes_instance() {
        let mut registry = InstanceRegistry::default();
        registry.add(sample_instance("docker:remove")).expect("add");
        let removed = registry.remove("docker:remove");
        assert!(removed.is_some());
        assert!(registry.get("docker:remove").is_none());
    }

    #[test]
    fn get_returns_instance_by_id() {
        let mut registry = InstanceRegistry::default();
        registry.add(sample_instance("docker:get")).expect("add");
        let instance = registry.get("docker:get");
        assert!(instance.is_some());
    }
}
