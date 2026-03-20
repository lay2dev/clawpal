use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone)]
pub struct OpenclawCli {
    bin: String,
}

#[derive(Debug, Error)]
pub enum OpenclawError {
    #[error("failed to run openclaw: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("openclaw command failed ({exit_code}): {details}")]
    CommandFailed { exit_code: i32, details: String },
    #[error("no json found in output: {0}")]
    NoJson(String),
    #[error("failed to parse json: {0}")]
    ParseJson(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, OpenclawError>;

pub fn resolve_openclaw_bin() -> &'static str {
    static BIN: OnceLock<String> = OnceLock::new();
    BIN.get_or_init(|| {
        if find_in_path("openclaw") {
            return "openclaw".to_string();
        }

        let home = std::env::var("HOME").unwrap_or_default();
        let candidates = [
            "/opt/homebrew/bin/openclaw".to_string(),
            "/usr/local/bin/openclaw".to_string(),
            format!("{home}/.npm-global/bin/openclaw"),
            format!("{home}/.local/bin/openclaw"),
        ];

        let nvm_dir = std::env::var("NVM_DIR").unwrap_or_else(|_| format!("{home}/.nvm"));
        let nvm_pattern = format!("{nvm_dir}/versions/node");
        let mut nvm_candidates = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&nvm_pattern) {
            for entry in entries.flatten() {
                let path = entry.path().join("bin/openclaw");
                if path.exists() {
                    nvm_candidates.push(path.to_string_lossy().to_string());
                }
            }
        }

        for candidate in candidates.iter().chain(nvm_candidates.iter()) {
            if Path::new(candidate).is_file() {
                if let Some(dir) = Path::new(candidate).parent() {
                    if let Ok(current_path) = std::env::var("PATH") {
                        let dir_str = dir.to_string_lossy();
                        let already_in_path = std::env::split_paths(&current_path)
                            .any(|path| path == Path::new(dir_str.as_ref()));
                        if !already_in_path {
                            std::env::set_var("PATH", format!("{dir_str}:{current_path}"));
                        }
                    }
                }
                return candidate.clone();
            }
        }

        "openclaw".to_string()
    })
}

impl OpenclawCli {
    pub fn new() -> Self {
        Self {
            bin: resolve_openclaw_bin().to_string(),
        }
    }

    pub fn with_bin(bin: impl Into<String>) -> Self {
        Self { bin: bin.into() }
    }

    pub fn run(&self, args: &[&str]) -> Result<CliOutput> {
        self.run_with_env(args, None)
    }

    pub fn run_with_env(
        &self,
        args: &[&str],
        env: Option<&HashMap<String, String>>,
    ) -> Result<CliOutput> {
        let mut cmd = Command::new(&self.bin);
        cmd.args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        if let Some(env_vars) = env {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        // Retry once on ETXTBSY (errno 26, "Text file busy"). This transient
        // error can occur when the binary was just written/updated (e.g. during
        // npm install or in tests under llvm-cov instrumentation).
        let output = match cmd.output() {
            Err(e) if e.raw_os_error() == Some(26) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
                Command::new(&self.bin)
                    .args(args)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .envs(env.iter().flat_map(|m| m.iter()))
                    .output()?
            }
            other => other?,
        };
        Ok(CliOutput {
            stdout: String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string(),
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

impl Default for OpenclawCli {
    fn default() -> Self {
        Self::new()
    }
}

/// Strip ANSI escape sequences (e.g. `\x1b[35m`) that plugin loggers may
/// leak into stdout.  The `]` inside these codes confuses the bracket-matching
/// JSON extractor.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Consume `[` + parameter bytes + final byte
            if let Some(next) = chars.next() {
                if next == '[' {
                    for c in chars.by_ref() {
                        // Final byte of a CSI sequence is in 0x40..=0x7E
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn parse_json_output(output: &CliOutput) -> Result<Value> {
    if output.exit_code != 0 {
        let details = if !output.stderr.is_empty() {
            output.stderr.clone()
        } else {
            output.stdout.clone()
        };
        return Err(OpenclawError::CommandFailed {
            exit_code: output.exit_code,
            details,
        });
    }

    let raw = &strip_ansi(&output.stdout);

    // Scan forward for balanced `[\xe2\x80\xa6]` or `{\xe2\x80\xa6}` candidates and try to parse
    // each one.  This handles noise both *before* and *after* the real JSON
    // payload (e.g. `[plugins] booting\n{"ok":true}\n[plugins] done`).
    let mut search_from = 0usize;
    loop {
        let first_brace = raw[search_from..].find('{').map(|i| i + search_from);
        let first_bracket = raw[search_from..].find('[').map(|i| i + search_from);
        let start = match (first_brace, first_bracket) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => return Err(OpenclawError::NoJson(raw.to_string())),
        };
        let opener = raw.as_bytes()[start];
        let closer = if opener == b'[' { b']' } else { b'}' };
        let mut depth = 0i32;
        let mut end = None;
        let mut in_string = false;
        let mut escape_next = false;
        for (i, &ch) in raw.as_bytes()[start..].iter().enumerate() {
            if escape_next {
                escape_next = false;
                continue;
            }
            if ch == b'\\' && in_string {
                escape_next = true;
                continue;
            }
            if ch == b'"' {
                in_string = !in_string;
                continue;
            }
            if in_string {
                continue;
            }
            if ch == opener {
                depth += 1;
            } else if ch == closer {
                depth -= 1;
            }
            if depth == 0 {
                end = Some(start + i);
                break;
            }
        }

        let end = match end {
            Some(e) => e,
            // Unbalanced \xe2\x80\x94 skip past this opener and try the next candidate.
            None => {
                search_from = start + 1;
                continue;
            }
        };
        let json_str = &raw[start..=end];
        match serde_json::from_str(json_str) {
            Ok(value) => return Ok(value),
            Err(_) => {
                // Not valid JSON (e.g. `[plugins]`), skip and try next.
                search_from = end + 1;
                continue;
            }
        }
    }
}

fn find_in_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(bin).is_file()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    #[cfg(unix)]
    fn create_fake_openclaw_script(body: &str) -> String {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let dir =
            std::env::temp_dir().join(format!("clawpal-core-openclaw-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("fake-openclaw.sh");
        // Open → write → fsync → close explicitly to avoid ETXTBSY on exec.
        {
            let mut f = fs::File::create(&path).expect("create script");
            f.write_all(body.as_bytes()).expect("write script");
            f.sync_all().expect("sync script");
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path.to_string_lossy().to_string()
    }

    #[test]
    fn resolve_openclaw_bin_returns_non_empty_path() {
        assert!(!resolve_openclaw_bin().is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn run_executes_binary_and_returns_output() {
        let bin = create_fake_openclaw_script("#!/bin/sh\necho '{\"ok\":true}'\n");
        let cli = OpenclawCli::with_bin(bin);
        let output = cli.run(&["status"]).expect("run");
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("\"ok\":true"));
    }

    #[test]
    #[cfg(unix)]
    fn run_with_env_passes_environment_variables() {
        let bin = create_fake_openclaw_script("#!/bin/sh\necho \"$CLAWPAL_TEST_ENV\"\n");
        let cli = OpenclawCli::with_bin(bin);
        let mut env = HashMap::new();
        env.insert("CLAWPAL_TEST_ENV".to_string(), "hello".to_string());
        let output = cli.run_with_env(&[], Some(&env)).expect("run_with_env");
        assert_eq!(output.stdout, "hello");
    }

    #[test]
    fn parse_json_output_extracts_payload_with_noise() {
        let output = CliOutput {
            stdout: "warn line\n{\"a\":1}".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        let value = parse_json_output(&output).expect("parse");
        assert_eq!(value["a"], 1);
    }

    #[test]
    fn parse_json_output_extracts_array() {
        let output = CliOutput {
            stdout: "some noise\n[{\"x\":1},{\"x\":2}]\nmore noise".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        let value = parse_json_output(&output).expect("parse");
        assert!(value.is_array());
        assert_eq!(value.as_array().unwrap().len(), 2);
    }

    #[test]
    fn parse_json_output_returns_error_on_nonzero_exit() {
        let output = CliOutput {
            stdout: String::new(),
            stderr: "command not found".to_string(),
            exit_code: 1,
        };
        let err = parse_json_output(&output).unwrap_err();
        match err {
            OpenclawError::CommandFailed { exit_code, details } => {
                assert_eq!(exit_code, 1);
                assert!(details.contains("command not found"));
            }
            _ => panic!("expected CommandFailed"),
        }
    }

    #[test]
    fn parse_json_output_uses_stdout_when_stderr_empty() {
        let output = CliOutput {
            stdout: "some error output".to_string(),
            stderr: String::new(),
            exit_code: 2,
        };
        let err = parse_json_output(&output).unwrap_err();
        assert!(err.to_string().contains("some error output"));
    }

    #[test]
    fn parse_json_output_no_json_returns_error() {
        let output = CliOutput {
            stdout: "just plain text without any json".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        let err = parse_json_output(&output).unwrap_err();
        assert!(matches!(err, OpenclawError::NoJson(_)));
    }

    #[test]
    fn parse_json_output_handles_ansi_codes_in_stdout() {
        // Reproduce the real-world scenario where feishu plugin logs with
        // ANSI color codes leak into stdout alongside JSON output.
        let output = CliOutput {
            stdout: "[{\"id\":\"main\"}]\n\x1b[35m[plugins]\x1b[39m \x1b[36mfeishu: ok\x1b[39m".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        let value = parse_json_output(&output).expect("parse with ANSI");
        assert!(value.is_array());
        assert_eq!(value[0]["id"], "main");
    }

    #[test]
    fn parse_json_output_skips_non_json_brackets_before_payload() {
        // Plugin log lines like "[plugins] booting" appear before the real
        // JSON payload — the extractor must skip them.
        let output = CliOutput {
            stdout: "[plugins] booting\n{"ok":true}\n[plugins] done".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        let value = parse_json_output(&output).expect("skip non-json prefix");
        assert_eq!(value, serde_json::json!({"ok": true}));
    }

        #[test]
    fn strip_ansi_removes_escape_sequences() {
        let input = "\x1b[35m[plugins]\x1b[39m hello";
        let cleaned = strip_ansi(input);
        assert_eq!(cleaned, "[plugins] hello");
        assert!(!cleaned.contains('\x1b'));
    }

    #[test]
    fn parse_json_output_nested_json() {
        let output = CliOutput {
            stdout: "{\"a\":{\"b\":{\"c\":42}}}".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        let value = parse_json_output(&output).expect("parse");
        assert_eq!(value["a"]["b"]["c"], 42);
    }

    #[test]
    fn cli_output_default_fields() {
        let cli = OpenclawCli::with_bin("echo");
        assert_eq!(cli.bin, "echo");
    }

    #[test]
    fn openclaw_error_display() {
        let err = OpenclawError::NoJson("no brackets".to_string());
        assert!(err.to_string().contains("no json"));

        let err = OpenclawError::CommandFailed {
            exit_code: 42,
            details: "bad".to_string(),
        };
        assert!(err.to_string().contains("42"));
    }
}
