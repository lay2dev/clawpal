use super::*;

#[tauri::command]
pub async fn remote_get_watchdog_status(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Value, String> {
    let status_raw = pool
        .sftp_read(&host_id, "~/.clawpal/watchdog/status.json")
        .await
        .unwrap_or_default();

    let pid_raw = pool
        .sftp_read(&host_id, "~/.clawpal/watchdog/watchdog.pid")
        .await;
    let alive_output = match pid_raw {
        Ok(pid_str) => {
            let cmd = format!(
                "kill -0 {} 2>/dev/null && echo alive || echo dead",
                pid_str.trim()
            );
            pool.exec(&host_id, &cmd)
                .await
                .map(|r| r.stdout)
                .unwrap_or_else(|_| "dead".to_string())
        }
        Err(_) => "dead".to_string(),
    };

    let deployed = pool
        .sftp_read(&host_id, "~/.clawpal/watchdog/watchdog.js")
        .await
        .is_ok();

    let mut status = clawpal_core::watchdog::parse_watchdog_status(&status_raw, &alive_output).extra;
    status.insert("deployed".into(), Value::Bool(deployed));
    Ok(Value::Object(status))
}



#[tauri::command]
pub async fn remote_deploy_watchdog(
    app_handle: tauri::AppHandle,
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<bool, String> {
    let resource_path = app_handle
        .path()
        .resolve(
            "resources/watchdog.js",
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| format!("Failed to resolve watchdog resource: {e}"))?;
    let content = std::fs::read_to_string(&resource_path)
        .map_err(|e| format!("Failed to read watchdog resource: {e}"))?;

    pool.exec(&host_id, "mkdir -p ~/.clawpal/watchdog").await?;
    pool.sftp_write(&host_id, "~/.clawpal/watchdog/watchdog.js", &content)
        .await?;
    Ok(true)
}



#[tauri::command]
pub async fn remote_start_watchdog(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<bool, String> {
    let pid_raw = pool
        .sftp_read(&host_id, "~/.clawpal/watchdog/watchdog.pid")
        .await;
    if let Ok(pid_str) = pid_raw {
        let cmd = format!(
            "kill -0 {} 2>/dev/null && echo alive || echo dead",
            pid_str.trim()
        );
        if let Ok(r) = pool.exec(&host_id, &cmd).await {
            if r.stdout.trim() == "alive" {
                return Ok(true);
            }
        }
    }

    let cmd = "cd ~/.clawpal/watchdog && nohup node watchdog.js >> watchdog.log 2>&1 &";
    pool.exec(&host_id, cmd).await?;
    // watchdog.js writes its own PID file to ~/.clawpal/watchdog/
    Ok(true)
}



#[tauri::command]
pub async fn remote_stop_watchdog(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<bool, String> {
    let pid_raw = pool
        .sftp_read(&host_id, "~/.clawpal/watchdog/watchdog.pid")
        .await;
    if let Ok(pid_str) = pid_raw {
        let _ = pool
            .exec(&host_id, &format!("kill {} 2>/dev/null", pid_str.trim()))
            .await;
    }
    let _ = pool
        .exec(&host_id, "rm -f ~/.clawpal/watchdog/watchdog.pid")
        .await;
    Ok(true)
}



#[tauri::command]
pub async fn remote_uninstall_watchdog(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<bool, String> {
    // Stop first
    let pid_raw = pool
        .sftp_read(&host_id, "~/.clawpal/watchdog/watchdog.pid")
        .await;
    if let Ok(pid_str) = pid_raw {
        let _ = pool
            .exec(&host_id, &format!("kill {} 2>/dev/null", pid_str.trim()))
            .await;
    }
    // Remove entire directory
    let _ = pool.exec(&host_id, "rm -rf ~/.clawpal/watchdog").await;
    Ok(true)
}
