use super::*;

#[tauri::command]
pub async fn remote_get_watchdog_status(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Value, String> {
    timed_async!("remote_get_watchdog_status", {
        let status_raw = pool
            .exec(
                &host_id,
                "cat ~/.clawpal/watchdog/status.json 2>/dev/null || true",
            )
            .await
            .map(|result| result.stdout)
            .unwrap_or_default();
        let probe = pool.exec(
            &host_id,
            "pid=\"\"; [ -f ~/.clawpal/watchdog/watchdog.pid ] && pid=$(cat ~/.clawpal/watchdog/watchdog.pid 2>/dev/null | tr -d '\\r\\n'); alive=dead; [ -n \"$pid\" ] && kill -0 \"$pid\" 2>/dev/null && alive=alive; deployed=0; [ -f ~/.clawpal/watchdog/watchdog.js ] && deployed=1; printf \"%s\\t%s\\t%s\\n\" \"$pid\" \"$alive\" \"$deployed\"",
        )
        .await
        .map(|result| result.stdout)
        .unwrap_or_default();
        let mut fields = probe.trim().splitn(3, '\t');
        let _pid = fields.next().unwrap_or("").trim();
        let alive_output = fields.next().unwrap_or("dead").to_string();
        let deployed = fields.next().map(|v| v.trim() == "1").unwrap_or(false);

        let mut status =
            clawpal_core::watchdog::parse_watchdog_status(&status_raw, &alive_output).extra;
        status.insert("deployed".into(), Value::Bool(deployed));
        Ok(Value::Object(status))
    })
}

#[tauri::command]
pub async fn remote_deploy_watchdog(
    app_handle: tauri::AppHandle,
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<bool, String> {
    timed_async!("remote_deploy_watchdog", {
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
    })
}

#[tauri::command]
pub async fn remote_start_watchdog(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<bool, String> {
    timed_async!("remote_start_watchdog", {
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
    })
}

#[tauri::command]
pub async fn remote_stop_watchdog(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<bool, String> {
    timed_async!("remote_stop_watchdog", {
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
    })
}

#[tauri::command]
pub async fn remote_uninstall_watchdog(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<bool, String> {
    timed_async!("remote_uninstall_watchdog", {
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
    })
}
