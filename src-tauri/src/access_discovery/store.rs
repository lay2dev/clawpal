use super::types::{CapabilityProfile, ExecutionExperience};
use crate::config_io::{read_text, write_text};
use std::path::{Path, PathBuf};

const MAX_EXPERIENCES_PER_INSTANCE: usize = 5;

#[derive(Clone, Debug)]
pub struct AccessDiscoveryStore {
    root_dir: PathBuf,
}

impl AccessDiscoveryStore {
    pub fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    pub fn from_path(root_dir: &Path) -> Self {
        Self {
            root_dir: root_dir.to_path_buf(),
        }
    }

    pub fn save_profile(&self, profile: &CapabilityProfile) -> Result<(), String> {
        let path = self.profile_path(&profile.instance_id);
        let payload = serde_json::to_string_pretty(profile).map_err(|e| e.to_string())?;
        write_text(&path, &payload)
    }

    pub fn load_profile(&self, instance_id: &str) -> Result<Option<CapabilityProfile>, String> {
        let path = self.profile_path(instance_id);
        if !path.exists() {
            return Ok(None);
        }
        let text = read_text(&path)?;
        let parsed = serde_json::from_str::<CapabilityProfile>(&text).map_err(|e| e.to_string())?;
        Ok(Some(parsed))
    }

    pub fn profile_path(&self, instance_id: &str) -> PathBuf {
        let name = sanitize_instance_id(instance_id);
        self.root_dir.join(format!("{name}.json"))
    }

    pub fn save_experience(&self, experience: ExecutionExperience) -> Result<usize, String> {
        let mut all = self.load_experiences(&experience.instance_id)?;
        if let Some(existing_idx) = all.iter().position(|item| {
            item.goal == experience.goal
                && item.method == experience.method
                && item.transport == experience.transport
        }) {
            // Update matched operation in place and move to tail as most recent.
            all.remove(existing_idx);
        }
        all.push(experience.clone());
        if all.len() > MAX_EXPERIENCES_PER_INSTANCE {
            let drop_count = all.len() - MAX_EXPERIENCES_PER_INSTANCE;
            all.drain(0..drop_count);
        }
        let path = self.experience_path(&experience.instance_id);
        let payload = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
        write_text(&path, &payload)?;
        Ok(all.len())
    }

    pub fn load_experiences(&self, instance_id: &str) -> Result<Vec<ExecutionExperience>, String> {
        let path = self.experience_path(instance_id);
        if !path.exists() {
            return Ok(vec![]);
        }
        let text = read_text(&path)?;
        serde_json::from_str::<Vec<ExecutionExperience>>(&text).map_err(|e| e.to_string())
    }

    pub fn experience_path(&self, instance_id: &str) -> PathBuf {
        let name = sanitize_instance_id(instance_id);
        self.root_dir.join(format!("{name}.experiences.json"))
    }
}

fn sanitize_instance_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    trimmed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == ':' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
