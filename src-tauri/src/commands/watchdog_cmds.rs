use serde_json::Value;
use tauri::Manager;

use crate::models::resolve_paths;

#[tauri::command]
pub async fn get_watchdog_status() -> Result<Value, String> {
    timed_async!("get_watchdog_status", {
        tauri::async_runtime::spawn_blocking(|| {
            let paths = resolve_paths();
            let wd_dir = paths.clawpal_dir.join("watchdog");
            let status_path = wd_dir.join("status.json");
            let pid_path = wd_dir.join("watchdog.pid");

            let mut status = if status_path.exists() {
                let text = std::fs::read_to_string(&status_path).map_err(|e| e.to_string())?;
                serde_json::from_str::<Value>(&text).unwrap_or(Value::Null)
            } else {
                Value::Null
            };

            let alive = if pid_path.exists() {
                let pid_str = std::fs::read_to_string(&pid_path).unwrap_or_default();
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    std::process::Command::new("kill")
                        .args(["-0", &pid.to_string()])
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false)
                } else {
                    false
                }
            } else {
                false
            };

            if let Value::Object(ref mut map) = status {
                map.insert("alive".into(), Value::Bool(alive));
                map.insert(
                    "deployed".into(),
                    Value::Bool(wd_dir.join("watchdog.js").exists()),
                );
            } else {
                let mut map = serde_json::Map::new();
                map.insert("alive".into(), Value::Bool(alive));
                map.insert(
                    "deployed".into(),
                    Value::Bool(wd_dir.join("watchdog.js").exists()),
                );
                status = Value::Object(map);
            }

            Ok(status)
        })
        .await
        .map_err(|e| e.to_string())?
    })
}

#[tauri::command]
pub fn deploy_watchdog(app_handle: tauri::AppHandle) -> Result<bool, String> {
    timed_sync!("deploy_watchdog", {
        let paths = resolve_paths();
        let wd_dir = paths.clawpal_dir.join("watchdog");
        std::fs::create_dir_all(&wd_dir).map_err(|e| e.to_string())?;

        let resource_path = app_handle
            .path()
            .resolve(
                "resources/watchdog.js",
                tauri::path::BaseDirectory::Resource,
            )
            .map_err(|e| format!("Failed to resolve watchdog resource: {e}"))?;

        let content = std::fs::read_to_string(&resource_path)
            .map_err(|e| format!("Failed to read watchdog resource: {e}"))?;

        std::fs::write(wd_dir.join("watchdog.js"), content).map_err(|e| e.to_string())?;
        crate::logging::log_info("Watchdog deployed");
        Ok(true)
    })
}

#[tauri::command]
pub fn start_watchdog() -> Result<bool, String> {
    timed_sync!("start_watchdog", {
        let paths = resolve_paths();
        let wd_dir = paths.clawpal_dir.join("watchdog");
        let script = wd_dir.join("watchdog.js");
        let pid_path = wd_dir.join("watchdog.pid");
        let log_path = wd_dir.join("watchdog.log");

        if !script.exists() {
            return Err("Watchdog not deployed. Deploy first.".into());
        }

        if pid_path.exists() {
            let pid_str = std::fs::read_to_string(&pid_path).unwrap_or_default();
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                let alive = std::process::Command::new("kill")
                    .args(["-0", &pid.to_string()])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if alive {
                    return Ok(true);
                }
            }
        }

        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| e.to_string())?;
        let log_err = log_file.try_clone().map_err(|e| e.to_string())?;

        let _child = std::process::Command::new("node")
            .arg(&script)
            .current_dir(&wd_dir)
            .env("CLAWPAL_WATCHDOG_DIR", &wd_dir)
            .stdout(log_file)
            .stderr(log_err)
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start watchdog: {e}"))?;

        // PID file is written by watchdog.js itself via acquirePidFile()
        crate::logging::log_info("Watchdog started");
        Ok(true)
    })
}

#[tauri::command]
pub fn stop_watchdog() -> Result<bool, String> {
    timed_sync!("stop_watchdog", {
        let paths = resolve_paths();
        let pid_path = paths.clawpal_dir.join("watchdog").join("watchdog.pid");

        if !pid_path.exists() {
            return Ok(true);
        }

        let pid_str = std::fs::read_to_string(&pid_path).unwrap_or_default();
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            let _ = std::process::Command::new("kill")
                .arg(pid.to_string())
                .output();
        }

        let _ = std::fs::remove_file(&pid_path);
        crate::logging::log_info("Watchdog stopped");
        Ok(true)
    })
}

#[tauri::command]
pub fn uninstall_watchdog() -> Result<bool, String> {
    timed_sync!("uninstall_watchdog", {
        let paths = resolve_paths();
        let wd_dir = paths.clawpal_dir.join("watchdog");

        // Stop first if running
        let pid_path = wd_dir.join("watchdog.pid");
        if pid_path.exists() {
            let pid_str = std::fs::read_to_string(&pid_path).unwrap_or_default();
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                let _ = std::process::Command::new("kill")
                    .arg(pid.to_string())
                    .output();
            }
        }

        // Remove entire watchdog directory
        if wd_dir.exists() {
            std::fs::remove_dir_all(&wd_dir).map_err(|e| e.to_string())?;
        }
        crate::logging::log_info("Watchdog uninstalled");
        Ok(true)
    })
}
