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

fn summarize_remote_config_payload(raw: &str) -> String {
    let parsed = serde_json::from_str::<serde_json::Value>(raw)
        .or_else(|_| json5::from_str::<serde_json::Value>(raw))
        .ok();
    let top_keys = parsed
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .map(|obj| {
            let mut keys = obj.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            keys.join(",")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "-".into());
    let provider_keys = parsed
        .as_ref()
        .and_then(|value| value.pointer("/models/providers"))
        .and_then(serde_json::Value::as_object)
        .map(|obj| {
            let mut keys = obj.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            keys.join(",")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "-".into());
    let agents_list_len = parsed
        .as_ref()
        .and_then(|value| value.pointer("/agents/list"))
        .and_then(serde_json::Value::as_array)
        .map(|list| list.len().to_string())
        .unwrap_or_else(|| "none".into());
    let defaults_workspace = parsed
        .as_ref()
        .and_then(|value| value.pointer("/agents/defaults/workspace"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("-");

    format!(
        "bytes={} top_keys=[{}] provider_keys=[{}] agents_list_len={} defaults_workspace={}",
        raw.len(),
        top_keys,
        provider_keys,
        agents_list_len,
        defaults_workspace,
    )
}

pub fn log_remote_config_write(
    action: &str,
    host_id: &str,
    source: Option<&str>,
    config_path: &str,
    raw: &str,
) {
    let source = source.unwrap_or("-");
    let summary = summarize_remote_config_payload(raw);
    log_dev(format!(
        "[dev][remote_config_write] action={action} host_id={host_id} source={source} config_path={config_path} {summary}"
    ));
}

pub fn log_remote_autofix_suppressed(host_id: &str, command: &str, reason: &str) {
    let command = command.replace('\n', " ");
    let reason = reason.replace('\n', " ");
    log_dev(format!(
        "[dev][remote_autofix_suppressed] host_id={host_id} command={command} reason={reason}"
    ));
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

#[cfg(test)]
mod tests {
    use super::summarize_remote_config_payload;

    #[test]
    fn summarize_valid_json_with_providers_and_agents() {
        let raw = r#"{
            "models": {"providers": {"openai": {}, "anthropic": {}}},
            "agents": {"list": [{"id": "a"}, {"id": "b"}], "defaults": {"workspace": "/home/user/ws"}}
        }"#;
        let summary = summarize_remote_config_payload(raw);
        assert!(summary.contains("provider_keys=[anthropic,openai]"), "{}", summary);
        assert!(summary.contains("agents_list_len=2"), "{}", summary);
        assert!(summary.contains("defaults_workspace=/home/user/ws"), "{}", summary);
    }

    #[test]
    fn summarize_invalid_json() {
        let summary = summarize_remote_config_payload("not json {{{");
        assert!(summary.contains("top_keys=[-]"), "{}", summary);
    }

    #[test]
    fn summarize_empty_json() {
        let summary = summarize_remote_config_payload("{}");
        assert!(summary.contains("top_keys=[-]"), "{}", summary);
        assert!(summary.contains("provider_keys=[-]"), "{}", summary);
        assert!(summary.contains("agents_list_len=none"), "{}", summary);
    }

    #[test]
    fn summarize_json_no_providers() {
        let raw = r#"{"models": {}}"#;
        let summary = summarize_remote_config_payload(raw);
        assert!(summary.contains("provider_keys=[-]"), "{}", summary);
    }
}
