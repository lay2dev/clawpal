use crate::json_util::extract_json_objects;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolIntent {
    pub tool: String,
    pub args: String,
    pub instance: Option<String>,
    pub reason: Option<String>,
}

pub fn classify_invoke_type(tool: &str, args: &str) -> &'static str {
    let tool_lc = tool.trim().to_ascii_lowercase();
    let args_lc = args.trim().to_ascii_lowercase();
    let is_prefix = |prefix: &str| args_lc == prefix || args_lc.starts_with(&format!("{prefix} "));

    if tool_lc == "clawpal" {
        let write_prefixes = [
            "instance remove",
            "profile add",
            "profile remove",
            "connect docker",
            "connect ssh",
            "install local",
            "install docker",
            "ssh connect",
            "ssh disconnect",
            "doctor exec",
            "doctor fix-openclaw-path",
            "doctor file write",
            "doctor config-upsert",
            "doctor config-delete",
            "doctor sessions-upsert",
            "doctor sessions-delete",
        ];
        if write_prefixes.iter().any(|p| is_prefix(p)) {
            return "write";
        }
        return "read";
    }

    if tool_lc == "openclaw" {
        let write_prefixes = [
            "doctor --fix",
            "config set",
            "config delete",
            "config unset",
            "auth add",
            "auth login",
            "auth remove",
            "gateway start",
            "gateway stop",
            "service install",
            "service uninstall",
            "service restart",
            "channel add",
            "channel remove",
            "channel update",
            "cron add",
            "cron remove",
            "cron update",
        ];
        if write_prefixes.iter().any(|p| is_prefix(p)) {
            return "write";
        }
        return "read";
    }

    // Unknown tool defaults to write for safety: it always requires explicit
    // user confirmation instead of auto-running as read.
    "write"
}

#[derive(Debug, Deserialize)]
struct ToolIntentPayload {
    tool: String,
    args: String,
    #[serde(default)]
    instance: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

fn extract_fenced_json(raw: &str) -> Option<String> {
    let marker = "```json";
    let start = raw.find(marker)?;
    let after = &raw[start + marker.len()..];
    let end = after.find("```")?;
    Some(after[..end].trim().to_string())
}

fn validate_payload(payload: ToolIntentPayload) -> Option<ToolIntent> {
    let tool = payload.tool.trim().to_string();
    if tool.is_empty() {
        return None;
    }
    let args = payload.args.trim().to_string();
    if args.is_empty() {
        return None;
    }
    Some(ToolIntent {
        tool,
        args,
        instance: payload
            .instance
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        reason: payload
            .reason
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
    })
}

pub fn parse_tool_intent(raw: &str) -> Option<ToolIntent> {
    let trimmed = raw.trim();
    let mut candidates = vec![trimmed.to_string()];
    if let Some(fenced) = extract_fenced_json(trimmed) {
        if fenced != trimmed {
            candidates.push(fenced);
        }
    }
    for extracted in extract_json_objects(trimmed) {
        if extracted != trimmed {
            candidates.push(extracted);
        }
    }

    for candidate in candidates {
        let Ok(payload) = serde_json::from_str::<ToolIntentPayload>(&candidate) else {
            continue;
        };
        if let Some(intent) = validate_payload(payload) {
            return Some(intent);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{classify_invoke_type, parse_tool_intent};

    #[test]
    fn parses_embedded_json_tool_intent() {
        let raw =
            "先检查。\n{\"tool\":\"clawpal\",\"args\":\"health check --all\",\"reason\":\"验证\"}";
        let intent = parse_tool_intent(raw).expect("intent");
        assert_eq!(intent.tool, "clawpal");
        assert_eq!(intent.args, "health check --all");
    }

    #[test]
    fn accepts_custom_tool() {
        let raw = "{\"tool\":\"bash\",\"args\":\"-lc \\\"echo 1\\\"\"}";
        let intent = parse_tool_intent(raw).expect("intent");
        assert_eq!(intent.tool, "bash");
    }

    #[test]
    fn parses_fenced_json() {
        let raw = "```json\n{\"tool\":\"openclaw\",\"args\":\"doctor --fix\"}\n```";
        let intent = parse_tool_intent(raw).expect("intent");
        assert_eq!(intent.tool, "openclaw");
        assert_eq!(intent.args, "doctor --fix");
    }

    #[test]
    fn classify_invoke_type_marks_mutations_as_write() {
        assert_eq!(
            classify_invoke_type("clawpal", "doctor file write --domain config --content {}"),
            "write"
        );
        assert_eq!(
            classify_invoke_type(
                "clawpal",
                "doctor exec --tool sudo --args \"rm -rf /tmp/x\""
            ),
            "write"
        );
        assert_eq!(classify_invoke_type("openclaw", "doctor --fix"), "write");
    }

    #[test]
    fn classify_invoke_type_marks_queries_as_read() {
        assert_eq!(
            classify_invoke_type("clawpal", "doctor file read --domain config"),
            "read"
        );
        assert_eq!(classify_invoke_type("openclaw", "gateway status"), "read");
    }

    #[test]
    fn classify_invoke_type_marks_unknown_tool_as_write() {
        assert_eq!(classify_invoke_type("bash", "-lc \"cat /tmp/x\""), "write");
    }
}
