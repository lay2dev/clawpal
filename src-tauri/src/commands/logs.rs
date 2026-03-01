use super::*;

fn clamp_lines(lines: Option<usize>) -> usize {
    lines.unwrap_or(200).clamp(1, 400)
}

#[tauri::command]
pub async fn remote_read_app_log(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    lines: Option<usize>,
) -> Result<String, String> {
    let n = clamp_lines(lines);
    let cmd = format!("tail -n {n} ~/.clawpal/logs/app.log 2>/dev/null || echo ''");
    let result = pool.exec(&host_id, &cmd).await?;
    Ok(result.stdout)
}

#[tauri::command]
pub async fn remote_read_error_log(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    lines: Option<usize>,
) -> Result<String, String> {
    let n = clamp_lines(lines);
    let cmd = format!("tail -n {n} ~/.clawpal/logs/error.log 2>/dev/null || echo ''");
    let result = pool.exec(&host_id, &cmd).await?;
    Ok(result.stdout)
}

#[tauri::command]
pub async fn remote_read_gateway_log(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    lines: Option<usize>,
) -> Result<String, String> {
    let n = clamp_lines(lines);
    let cmd = format!("tail -n {n} ~/.openclaw/logs/gateway.log 2>/dev/null || echo ''");
    let result = pool.exec(&host_id, &cmd).await?;
    Ok(result.stdout)
}

#[tauri::command]
pub async fn remote_read_gateway_error_log(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    lines: Option<usize>,
) -> Result<String, String> {
    let n = clamp_lines(lines);
    let cmd = format!("tail -n {n} ~/.openclaw/logs/gateway.err.log 2>/dev/null || echo ''");
    let result = pool.exec(&host_id, &cmd).await?;
    Ok(result.stdout)
}
