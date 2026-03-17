use super::*;

fn clamp_lines(lines: Option<usize>) -> usize {
    lines.unwrap_or(200).clamp(1, 400)
}

fn log_dev_enabled() -> bool {
    if cfg!(debug_assertions) {
        return true;
    }
    match std::env::var("CLAWPAL_DEV_LOG") {
        Ok(value) => {
            let value = value.to_ascii_lowercase();
            value == "1" || value == "true" || value == "yes" || value == "on"
        }
        Err(_) => false,
    }
}

pub fn log_dev(message: impl AsRef<str>) {
    if log_dev_enabled() {
        eprintln!("[dev] {}", message.as_ref());
    }
}

fn log_debug(message: &str) {
    log_dev(format!("[dev][logs] {message}"));
}

fn remote_gateway_log_command(lines: usize) -> String {
    let mut cmd = String::new();
    cmd.push_str("n=");
    cmd.push_str(&lines.to_string());
    cmd.push_str("; ");
    cmd.push_str(
        "gateway_data_root=\"${CLAWPAL_DATA_DIR:-${OPENCLAW_STATE_DIR:-${OPENCLAW_HOME:-$HOME/.openclaw}}}\"; ",
    );
    cmd.push_str("log_path=\"\"; ");
    cmd.push_str("for base in ");
    cmd.push_str("\"$gateway_data_root\" ");
    cmd.push_str("\"$gateway_data_root/.openclaw\" ");
    cmd.push_str("\"$gateway_data_root/.clawpal\" ");
    cmd.push_str("\"$OPENCLAW_STATE_DIR\" ");
    cmd.push_str("\"$OPENCLAW_STATE_DIR/.openclaw\" ");
    cmd.push_str("\"$OPENCLAW_STATE_DIR/.clawpal\" ");
    cmd.push_str("\"$OPENCLAW_HOME\" ");
    cmd.push_str("\"$OPENCLAW_HOME/.openclaw\" ");
    cmd.push_str("\"$OPENCLAW_HOME/.clawpal\" ");
    cmd.push_str("\"$HOME/.openclaw\" ");
    cmd.push_str("\"$HOME/.clawpal\"; ");
    cmd.push_str("do ");
    cmd.push_str("candidate=\"$base/logs/gateway.log\"; ");
    cmd.push_str("[ -f \"$candidate\" ] && log_path=\"$candidate\" && break; ");
    cmd.push_str("done; ");
    cmd.push_str("tmp_gateway_run=\"\"; [ -f \"/tmp/openclaw/gateway-run.log\" ] && tmp_gateway_run=\"/tmp/openclaw/gateway-run.log\"; ");
    cmd.push_str(
        "latest_openclaw_log=$(ls -1t /tmp/openclaw/openclaw-*.log 2>/dev/null | head -n 1); ",
    );
    cmd.push_str("found=0; ");
    cmd.push_str("if [ -n \"$log_path\" ] && [ -f \"$log_path\" ]; then printf '==> %s <==\\n' \"$log_path\"; tail -n \"$n\" \"$log_path\" 2>/dev/null; found=1; fi; ");
    cmd.push_str("if [ -n \"$tmp_gateway_run\" ] && [ \"$tmp_gateway_run\" != \"$log_path\" ]; then [ \"$found\" -eq 0 ] || printf '\\n'; printf '==> %s <==\\n' \"$tmp_gateway_run\"; tail -n \"$n\" \"$tmp_gateway_run\" 2>/dev/null; found=1; fi; ");
    cmd.push_str("if [ -n \"$latest_openclaw_log\" ] && [ \"$latest_openclaw_log\" != \"$log_path\" ] && [ \"$latest_openclaw_log\" != \"$tmp_gateway_run\" ]; then [ \"$found\" -eq 0 ] || printf '\\n'; printf '==> %s <==\\n' \"$latest_openclaw_log\"; tail -n \"$n\" \"$latest_openclaw_log\" 2>/dev/null; found=1; fi; ");
    cmd.push_str("[ \"$found\" -eq 1 ] || echo ''");
    cmd
}

#[tauri::command]
pub async fn remote_read_app_log(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    lines: Option<usize>,
) -> Result<String, String> {
    timed_async!("remote_read_app_log", {
        let n = clamp_lines(lines);
        let cmd = clawpal_core::doctor::remote_clawpal_log_tail_script(n, "app");
        log_debug(&format!(
            "remote_read_app_log start host_id={host_id} lines={n} cmd={cmd}"
        ));
        let result = pool.exec(&host_id, &cmd).await.map_err(|error| {
            log_debug(&format!(
                "remote_read_app_log failed host_id={host_id} error={error}"
            ));
            error
        })?;
        Ok(result.stdout)
    })
}

#[tauri::command]
pub async fn remote_read_error_log(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    lines: Option<usize>,
) -> Result<String, String> {
    timed_async!("remote_read_error_log", {
        let n = clamp_lines(lines);
        let cmd = clawpal_core::doctor::remote_clawpal_log_tail_script(n, "error");
        log_debug(&format!(
            "remote_read_error_log start host_id={host_id} lines={n} cmd={cmd}"
        ));
        let result = pool.exec(&host_id, &cmd).await.map_err(|error| {
            log_debug(&format!(
                "remote_read_error_log failed host_id={host_id} error={error}"
            ));
            error
        })?;
        Ok(result.stdout)
    })
}

#[tauri::command]
pub async fn remote_read_helper_log(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    lines: Option<usize>,
) -> Result<String, String> {
    timed_async!("remote_read_helper_log", {
        let n = clamp_lines(lines);
        let cmd = clawpal_core::doctor::remote_clawpal_log_tail_script(n, "helper");
        log_debug(&format!(
            "remote_read_helper_log start host_id={host_id} lines={n} cmd={cmd}"
        ));
        let result = pool.exec(&host_id, &cmd).await.map_err(|error| {
            log_debug(&format!(
                "remote_read_helper_log failed host_id={host_id} error={error}"
            ));
            error
        })?;
        Ok(result.stdout)
    })
}

#[tauri::command]
pub async fn remote_read_gateway_log(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    lines: Option<usize>,
) -> Result<String, String> {
    timed_async!("remote_read_gateway_log", {
        let n = clamp_lines(lines);
        let cmd = remote_gateway_log_command(n);
        log_debug(&format!(
            "remote_read_gateway_log start host_id={host_id} lines={n} cmd={cmd}"
        ));
        let result = pool.exec(&host_id, &cmd).await.map_err(|error| {
            log_debug(&format!(
                "remote_read_gateway_log failed host_id={host_id} error={error}"
            ));
            error
        })?;
        Ok(result.stdout)
    })
}

#[tauri::command]
pub async fn remote_read_gateway_error_log(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    lines: Option<usize>,
) -> Result<String, String> {
    timed_async!("remote_read_gateway_error_log", {
        let n = clamp_lines(lines);
        let cmd = clawpal_core::doctor::remote_gateway_error_log_tail_script(n);
        log_debug(&format!(
            "remote_read_gateway_error_log start host_id={host_id} lines={n} cmd={cmd}"
        ));
        let result = pool.exec(&host_id, &cmd).await.map_err(|error| {
            log_debug(&format!(
                "remote_read_gateway_error_log failed host_id={host_id} error={error}"
            ));
            error
        })?;
        Ok(result.stdout)
    })
}
