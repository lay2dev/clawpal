use super::*;

/// Escape a string for safe inclusion in a single-quoted shell argument.
pub(crate) fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub(crate) fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = std::env::var("HOME").ok() {
            return format!("{}{}", home, &path[1..]);
        }
    }
    path.to_string()
}

/// Clear cached openclaw version — call after upgrade so status shows new version.
pub fn clear_openclaw_version_cache() {
    *OPENCLAW_VERSION_CACHE.lock().unwrap() = None;
}

static OPENCLAW_VERSION_CACHE: std::sync::Mutex<Option<Option<String>>> =
    std::sync::Mutex::new(None);

pub(crate) fn resolve_openclaw_version() -> String {
    use std::sync::OnceLock;
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION
        .get_or_init(|| match run_openclaw_raw(&["--version"]) {
            Ok(output) => {
                extract_version_from_text(&output.stdout).unwrap_or_else(|| "unknown".into())
            }
            Err(_) => "unknown".into(),
        })
        .clone()
}

pub(crate) fn run_openclaw_dynamic(args: &[String]) -> Result<OpenclawCommandOutput, String> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::cli_runner::run_openclaw(&refs).map(Into::into)
}

pub(crate) fn run_openclaw_raw(args: &[&str]) -> Result<OpenclawCommandOutput, String> {
    run_openclaw_raw_timeout(args, None)
}

pub(crate) fn run_openclaw_raw_timeout(
    args: &[&str],
    timeout_secs: Option<u64>,
) -> Result<OpenclawCommandOutput, String> {
    let mut command = Command::new(clawpal_core::openclaw::resolve_openclaw_bin());
    command
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(path) = crate::cli_runner::get_active_openclaw_home_override() {
        command.env("OPENCLAW_HOME", path);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to run openclaw: {error}"))?;

    if let Some(secs) = timeout_secs {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        loop {
            match child.try_wait().map_err(|e| e.to_string())? {
                Some(status) => {
                    let mut stdout_buf = Vec::new();
                    let mut stderr_buf = Vec::new();
                    if let Some(mut out) = child.stdout.take() {
                        std::io::Read::read_to_end(&mut out, &mut stdout_buf).ok();
                    }
                    if let Some(mut err) = child.stderr.take() {
                        std::io::Read::read_to_end(&mut err, &mut stderr_buf).ok();
                    }
                    let exit_code = status.code().unwrap_or(-1);
                    let result = OpenclawCommandOutput {
                        stdout: String::from_utf8_lossy(&stdout_buf).trim_end().to_string(),
                        stderr: String::from_utf8_lossy(&stderr_buf).trim_end().to_string(),
                        exit_code,
                    };
                    if exit_code != 0 {
                        let details = if !result.stderr.is_empty() {
                            result.stderr.clone()
                        } else {
                            result.stdout.clone()
                        };
                        return Err(format!("openclaw command failed ({exit_code}): {details}"));
                    }
                    return Ok(result);
                }
                None => {
                    if std::time::Instant::now() >= deadline {
                        let _ = child.kill();
                        return Err(format!(
                            "Command timed out after {secs}s. The gateway may still be restarting in the background."
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            }
        }
    } else {
        let output = child
            .wait_with_output()
            .map_err(|error| format!("failed to run openclaw: {error}"))?;
        let exit_code = output.status.code().unwrap_or(-1);
        let result = OpenclawCommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string(),
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string(),
            exit_code,
        };
        if exit_code != 0 {
            let details = if !result.stderr.is_empty() {
                result.stderr.clone()
            } else {
                result.stdout.clone()
            };
            return Err(format!("openclaw command failed ({exit_code}): {details}"));
        }
        Ok(result)
    }
}

/// Extract the last JSON array from CLI output that may contain ANSI codes and plugin logs.
/// Scans from the end to find the last `]`, then finds its matching `[`.
pub(crate) fn extract_last_json_array(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    let end = bytes.iter().rposition(|&b| b == b']')?;
    let mut depth = 0;
    for i in (0..=end).rev() {
        match bytes[i] {
            b']' => depth += 1,
            b'[' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&raw[i..=end]);
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn parse_json_from_openclaw_output(output: &OpenclawCommandOutput) -> Option<Value> {
    clawpal_core::doctor::extract_json_from_output(&output.stdout)
        .and_then(|json| serde_json::from_str::<Value>(json).ok())
        .or_else(|| {
            clawpal_core::doctor::extract_json_from_output(&output.stderr)
                .and_then(|json| serde_json::from_str::<Value>(json).ok())
        })
}
