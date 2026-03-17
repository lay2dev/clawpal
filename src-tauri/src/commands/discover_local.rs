use std::collections::HashSet;
use std::process::Command;

use serde::Serialize;

use clawpal_core::instance::InstanceRegistry;

/// A Docker instance or data-dir discovered on the local machine.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredInstance {
    pub id: String,
    /// Always "docker" for now.
    pub instance_type: String,
    pub label: String,
    pub home_path: String,
    /// "container" if found via `docker ps`, "data_dir" if found via ~/.clawpal/ scan.
    pub source: String,
    pub container_name: Option<String>,
    pub already_registered: bool,
}

/// Convert a container name to a URL-safe slug.
///
/// Strips leading `/`, lowercases, and replaces non-alphanumeric chars with `-`.
#[cfg(test)]
fn slug_from_name(name: &str) -> String {
    let trimmed = name.strip_prefix('/').unwrap_or(name);
    let mut slug = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' {
            slug.push(ch.to_ascii_lowercase());
        } else {
            // Collapse repeated dashes
            if slug.ends_with('-') {
                continue;
            }
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

/// Discover local Docker instances that are either running as containers
/// or exist as data directories under `~/.clawpal/`.
#[tauri::command]
pub async fn discover_local_instances() -> Result<Vec<DiscoveredInstance>, String> {
    timed_async!("discover_local_instances", {
    tauri::async_runtime::spawn_blocking(|| discover_blocking())
        .await
        .map_err(|e| e.to_string())?
    })
}

fn discover_blocking() -> Result<Vec<DiscoveredInstance>, String> {
    // 1. Load registry for already_registered check
    let (registered_ids, registered_home_paths): (HashSet<String>, HashSet<String>) =
        InstanceRegistry::load()
            .map(|r| {
                let mut ids = HashSet::new();
                let mut homes = HashSet::new();
                for inst in r.list() {
                    ids.insert(inst.id);
                    if let Some(home) = inst.openclaw_home {
                        let key = normalize_home_path_for_match(&home);
                        if !key.is_empty() {
                            homes.insert(key);
                        }
                    }
                }
                (ids, homes)
            })
            .unwrap_or_default();

    let mut results: Vec<DiscoveredInstance> = Vec::new();
    let mut seen_home_paths: HashSet<String> = HashSet::new();

    // 2. Scan Docker containers
    if let Ok(containers) = scan_docker_containers() {
        for inst in containers {
            if seen_home_paths.contains(&inst.home_path) {
                continue;
            }
            seen_home_paths.insert(inst.home_path.clone());
            results.push(inst);
        }
    }

    // 3. Scan ~/.clawpal/ data directories
    if let Ok(data_dirs) = scan_data_dirs() {
        for inst in data_dirs {
            if seen_home_paths.contains(&inst.home_path) {
                continue;
            }
            seen_home_paths.insert(inst.home_path.clone());
            results.push(inst);
        }
    }

    // 4. Mark already_registered
    for inst in &mut results {
        let home_key = normalize_home_path_for_match(&inst.home_path);
        inst.already_registered =
            registered_ids.contains(&inst.id) || registered_home_paths.contains(&home_key);
    }

    Ok(results)
}

/// Normalize an OpenClaw home path for fuzzy matching between discovery and registry.
///
/// Handles:
/// - `~` / `~/...` expansion
/// - slash normalization
/// - trailing slash trimming
fn normalize_home_path_for_match(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return String::new();
    }
    if s == "~" {
        if let Some(home) = dirs::home_dir() {
            s = home.to_string_lossy().to_string();
        }
    } else if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            s = home.join(rest).to_string_lossy().to_string();
        }
    }
    let mut normalized = s.replace('\\', "/");
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }
    normalized
}

/// Run `docker ps --format '{{json .}}'` and parse matching containers.
fn scan_docker_containers() -> Result<Vec<DiscoveredInstance>, String> {
    let mut child = match Command::new("docker")
        .args(["ps", "--format", "{{json .}}"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return Ok(Vec::new()), // Docker not available
    };

    // 3-second timeout to avoid blocking if Docker daemon is hung
    let timeout = std::time::Duration::from_secs(3);
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Ok(Vec::new()); // Timed out
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => return Ok(Vec::new()),
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed to read docker ps output: {e}"))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut instances = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let container: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let names = container
            .get("Names")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let labels = container
            .get("Labels")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let names_lower = names.to_lowercase();
        let labels_lower = labels.to_lowercase();

        let is_match = names_lower.contains("openclaw")
            || names_lower.contains("clawpal")
            || labels_lower.contains("com.clawpal");

        if !is_match {
            continue;
        }

        // Try to extract home path from:
        // 1. Docker label com.clawpal.home
        // 2. Container env var OPENCLAW_CONFIG_DIR (via docker inspect)
        // 3. Container bind mounts to .clawpal directories
        let home_from_label = extract_label_value(labels, "com.clawpal.home");
        let container_id = container
            .get("ID")
            .and_then(|v| v.as_str())
            .unwrap_or(names);

        let home_path = home_from_label.or_else(|| inspect_container_home(container_id));

        // Skip containers where we can't determine a valid host-side path
        let Some(home_path) = home_path else {
            continue;
        };

        // Derive instance ID from the home path directory name (e.g. "docker-local")
        let dir_name = std::path::Path::new(&home_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("local");
        let slug = if dir_name.starts_with("docker-") {
            dir_name.strip_prefix("docker-").unwrap_or(dir_name)
        } else {
            dir_name
        };
        let id = format!("docker:{slug}");

        let label = names.strip_prefix('/').unwrap_or(names).to_string();

        instances.push(DiscoveredInstance {
            id,
            instance_type: "docker".to_string(),
            label,
            home_path,
            source: "container".to_string(),
            container_name: Some(names.to_string()),
            already_registered: false,
        });
    }

    Ok(instances)
}

/// Inspect a Docker container to find the host-side openclaw home path.
///
/// Checks (in order):
/// 1. `OPENCLAW_CONFIG_DIR` env var → parent of `.openclaw` dir
/// 2. Bind mounts whose source contains `.clawpal`
fn inspect_container_home(container_id: &str) -> Option<String> {
    let output = Command::new("docker")
        .args(["inspect", "--format", "{{json .}}", container_id])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let info: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;

    // 1. Check env vars for OPENCLAW_CONFIG_DIR
    if let Some(envs) = info.pointer("/Config/Env").and_then(|v| v.as_array()) {
        for env in envs {
            if let Some(s) = env.as_str() {
                if let Some(val) = s.strip_prefix("OPENCLAW_CONFIG_DIR=") {
                    // OPENCLAW_CONFIG_DIR points to the state dir (e.g. ~/.clawpal/docker-local/.openclaw)
                    // The home is the parent directory
                    let state_path = std::path::Path::new(val);
                    if let Some(parent) = state_path.parent() {
                        let home = parent.to_string_lossy().to_string();
                        if std::path::Path::new(&home).exists() {
                            return Some(home);
                        }
                    }
                    // If the config dir itself looks like a home dir (no .openclaw suffix)
                    if std::path::Path::new(val).exists() && !val.ends_with(".openclaw") {
                        return Some(val.to_string());
                    }
                }
            }
        }
    }

    // 2. Check bind mounts for .clawpal paths
    if let Some(mounts) = info.get("Mounts").and_then(|v| v.as_array()) {
        for mount in mounts {
            if let Some(source) = mount.get("Source").and_then(|v| v.as_str()) {
                if source.contains(".clawpal") && std::path::Path::new(source).exists() {
                    // Find the .clawpal/docker-* parent
                    let p = std::path::Path::new(source);
                    // Walk up to find the docker-* directory under .clawpal
                    let mut candidate = Some(p);
                    while let Some(c) = candidate {
                        if let Some(parent) = c.parent() {
                            if parent.file_name().and_then(|n| n.to_str()) == Some(".clawpal") {
                                return Some(c.to_string_lossy().to_string());
                            }
                        }
                        candidate = c.parent();
                    }
                    return Some(source.to_string());
                }
            }
        }
    }

    None
}

/// Extract a specific key from a Docker labels string (comma-separated key=value pairs).
fn extract_label_value(labels: &str, key: &str) -> Option<String> {
    for pair in labels.split(',') {
        let pair = pair.trim();
        if let Some(val) = pair.strip_prefix(key) {
            if let Some(val) = val.strip_prefix('=') {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// Scan `~/.clawpal/` for subdirectories starting with "docker-" that contain
/// `openclaw.json` or `docker-compose.yml`/`.yaml`.
fn scan_data_dirs() -> Result<Vec<DiscoveredInstance>, String> {
    let home = dirs::home_dir().ok_or("cannot determine home directory")?;
    let clawpal_dir = home.join(".clawpal");

    scan_data_dirs_under(&clawpal_dir)
}

fn scan_data_dirs_under(clawpal_dir: &std::path::Path) -> Result<Vec<DiscoveredInstance>, String> {
    if !clawpal_dir.is_dir() {
        return Ok(Vec::new());
    }

    let entries =
        std::fs::read_dir(&clawpal_dir).map_err(|e| format!("failed to read ~/.clawpal: {e}"))?;

    let mut instances = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("docker-") {
            continue;
        }

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let has_marker = path.join("openclaw.json").exists()
            || path.join(".openclaw").join("openclaw.json").exists()
            || path.join("docker-compose.yml").exists()
            || path.join("docker-compose.yaml").exists();

        if !has_marker {
            continue;
        }

        let slug = name.strip_prefix("docker-").unwrap_or(&name).to_string();
        let id = format!("docker:{slug}");
        let home_path = path.to_string_lossy().to_string();

        instances.push(DiscoveredInstance {
            id,
            instance_type: "docker".to_string(),
            label: slug.clone(),
            home_path,
            source: "data_dir".to_string(),
            container_name: None,
            already_registered: false,
        });
    }

    Ok(instances)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_from_name_basic() {
        assert_eq!(slug_from_name("/my-project"), "my-project");
        assert_eq!(slug_from_name("My Project"), "my-project");
        assert_eq!(slug_from_name("/openclaw_dev"), "openclaw-dev");
        assert_eq!(slug_from_name("///foo///bar"), "foo-bar");
    }

    #[test]
    fn extract_label_value_works() {
        let labels = "com.clawpal=true,com.clawpal.home=/data/oc,other=val";
        assert_eq!(
            extract_label_value(labels, "com.clawpal.home"),
            Some("/data/oc".to_string())
        );
        assert_eq!(extract_label_value(labels, "missing"), None);
    }

    #[test]
    fn normalize_home_path_for_match_trims_and_normalizes() {
        assert_eq!(
            normalize_home_path_for_match("/tmp/.clawpal/docker-local/"),
            "/tmp/.clawpal/docker-local".to_string()
        );
        assert_eq!(
            normalize_home_path_for_match("C:\\tmp\\.clawpal\\docker-local\\"),
            "C:/tmp/.clawpal/docker-local".to_string()
        );
    }

    #[test]
    fn scan_data_dirs_detects_openclaw_config_under_dot_openclaw() {
        let root =
            std::env::temp_dir().join(format!("clawpal-discover-local-{}", uuid::Uuid::new_v4()));
        let docker_dir = root.join("docker-local");
        std::fs::create_dir_all(docker_dir.join(".openclaw")).expect("create docker dir");
        std::fs::write(docker_dir.join(".openclaw").join("openclaw.json"), "{}")
            .expect("write openclaw config");

        let items = scan_data_dirs_under(&root).expect("scan data dirs");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "docker:local");
        assert_eq!(items[0].source, "data_dir");

        let _ = std::fs::remove_dir_all(root);
    }
}
