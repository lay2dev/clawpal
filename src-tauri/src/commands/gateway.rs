use super::*;

#[tauri::command]
pub async fn remote_restart_gateway(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<bool, String> {
    timed_async!("remote_restart_gateway", {
        pool.exec_login(&host_id, "openclaw gateway restart")
            .await?;
        Ok(true)
    })
}

#[tauri::command]
pub async fn restart_gateway() -> Result<bool, String> {
    timed_async!("restart_gateway", {
        tauri::async_runtime::spawn_blocking(move || {
            run_openclaw_raw(&["gateway", "restart"])?;
            Ok(true)
        })
        .await
        .map_err(|e| e.to_string())?
    })
}
