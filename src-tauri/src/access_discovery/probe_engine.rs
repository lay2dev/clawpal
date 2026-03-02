use super::types::ProbeResult;

pub fn build_probe_plan_for_local() -> Vec<String> {
    vec![
        "openclaw --version".to_string(),
        "/opt/homebrew/bin/openclaw --version".to_string(),
        "/usr/local/bin/openclaw --version".to_string(),
        "openclaw status".to_string(),
    ]
}

pub fn run_probe_with_redaction(
    probe_id: &str,
    command: &str,
    output: &str,
    ok: bool,
    elapsed_ms: u64,
) -> ProbeResult {
    ProbeResult {
        probe_id: probe_id.to_string(),
        command: command.to_string(),
        ok,
        summary: redact_sensitive(output),
        elapsed_ms,
    }
}

fn redact_sensitive(raw: &str) -> String {
    let mut result = raw.to_string();
    for marker in ["api_key", "token", "secret", "password", "bearer"] {
        if result.to_lowercase().contains(marker) {
            result = "[redacted sensitive output]".to_string();
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_sensitive_replaces_secret_like_content() {
        let out = redact_sensitive("api_key=sk-123");
        assert_eq!(out, "[redacted sensitive output]");
    }
}
