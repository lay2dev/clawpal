use crate::doctor::classify_engine_error;
use crate::runtime::types::{
    RuntimeAdapter, RuntimeError, RuntimeErrorCode, RuntimeEvent, RuntimeSessionKey,
};
use serde_json::json;
use serde_json::Value;

use super::process::run_zeroclaw_message;
use super::session::{append_history, build_prompt_with_history, reset_history};

pub struct ZeroclawDoctorAdapter;

impl ZeroclawDoctorAdapter {
    fn extract_json_objects(raw: &str) -> Vec<String> {
        let bytes = raw.as_bytes();
        let mut out = Vec::new();
        let mut start: Option<usize> = None;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for (i, b) in bytes.iter().enumerate() {
            if in_string {
                if escaped {
                    escaped = false;
                    continue;
                }
                if *b == b'\\' {
                    escaped = true;
                    continue;
                }
                if *b == b'"' {
                    in_string = false;
                }
                continue;
            }
            if *b == b'"' {
                in_string = true;
                continue;
            }
            if *b == b'{' {
                if start.is_none() {
                    start = Some(i);
                }
                depth += 1;
                continue;
            }
            if *b == b'}' {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        out.push(raw[s..=i].to_string());
                        start = None;
                    }
                }
            }
        }
        out
    }

    fn doctor_domain_prompt(key: &RuntimeSessionKey, message: &str) -> String {
        let target_line = if key.instance_id == "local" {
            "Target is local machine."
        } else {
            "Target is a non-local instance selected in ClawPal."
        };
        let target_id_line = format!("Target instance id: {}", key.instance_id);
        [
            "DOCTOR DOMAIN ONLY.",
            "You are ClawPal Doctor assistant.",
            "Identity rule: you are Doctor Claw (engine), not the target host.",
            "If user asks who/where you are, include both engine and target instance id.",
            "Do NOT infer transport type from instance name pattern.",
            "Use the provided context to decide whether target is local/docker/remote.",
            "Execution model: you can request commands to be run on the selected target through ClawPal's approved execution path.",
            "If command execution is needed, output ONLY JSON:",
            "{\"tool\":\"clawpal\",\"args\":\"<subcommand>\",\"reason\":\"<why>\"}",
            "{\"tool\":\"openclaw\",\"args\":\"<subcommand>\",\"instance\":\"<optional instance id>\",\"reason\":\"<why>\"}",
            "Do NOT claim you cannot access remote host due to missing SSH in your environment.",
            "Do NOT ask user to run commands manually when diagnosis requires commands.",
            "Do NOT output install/orchestrator JSON such as {\"step\":..., \"reason\":...}.",
            "Always answer in plain natural language with diagnosis and next actions.",
            target_line,
            &target_id_line,
            "",
            message,
        ]
        .join("\n")
    }

    fn normalize_doctor_output(raw: String) -> String {
        let trimmed = raw.trim();
        let mut candidates = vec![trimmed.to_string()];
        for extracted in Self::extract_json_objects(trimmed) {
            if extracted != trimmed {
                candidates.push(extracted);
            }
        }
        for candidate in candidates {
            if let Ok(v) = serde_json::from_str::<Value>(&candidate) {
                let step = v.get("step").and_then(|x| x.as_str());
                let reason = v.get("reason").and_then(|x| x.as_str());
                if step.is_some() && reason.is_some() {
                    return format!(
                        "当前是 Doctor 诊断模式，不执行安装编排。诊断建议：{}",
                        reason.unwrap_or("请先收集错误日志并确认运行状态。")
                    );
                }
            }
        }
        raw
    }

    fn parse_tool_intent(raw: &str) -> Option<(RuntimeEvent, String)> {
        let trimmed = raw.trim();
        let mut candidates = vec![trimmed.to_string()];
        for extracted in Self::extract_json_objects(trimmed) {
            if extracted != trimmed {
                candidates.push(extracted);
            }
        }
        for candidate in candidates {
            if let Ok(v) = serde_json::from_str::<Value>(&candidate) {
                let tool = v.get("tool").and_then(|x| x.as_str());
                if tool == Some("clawpal") || tool == Some("openclaw") {
                    let args = v.get("args")?.as_str()?.trim().to_string();
                    if args.is_empty() {
                        return None;
                    }
                    let reason = v
                        .get("reason")
                        .and_then(|x| x.as_str())
                        .unwrap_or("需要执行命令以继续诊断。")
                        .to_string();
                    let payload = json!({
                        "id": format!("zc-{}", uuid::Uuid::new_v4()),
                        "command": tool.unwrap_or("clawpal"),
                        "args": {
                            "args": args,
                            "instance": v.get("instance").and_then(|x| x.as_str()).unwrap_or(""),
                        },
                        "type": "read",
                    });
                    let note = format!(
                        "建议执行诊断命令：`{} {}`\n原因：{}",
                        payload["command"].as_str().unwrap_or(""),
                        payload["args"]["args"].as_str().unwrap_or(""),
                        reason
                    );
                    return Some((RuntimeEvent::Invoke { payload }, note));
                }
            }
        }
        None
    }

    fn map_error(err: String) -> RuntimeError {
        let code = match classify_engine_error(&err) {
            "CONFIG_MISSING" => RuntimeErrorCode::ConfigMissing,
            "MODEL_UNAVAILABLE" => RuntimeErrorCode::ModelUnavailable,
            "RUNTIME_UNREACHABLE" => RuntimeErrorCode::RuntimeUnreachable,
            _ => RuntimeErrorCode::Unknown,
        };
        RuntimeError {
            code,
            message: err,
            action_hint: None,
        }
    }
}

impl RuntimeAdapter for ZeroclawDoctorAdapter {
    fn engine_name(&self) -> &'static str {
        "zeroclaw"
    }

    fn start(
        &self,
        key: &RuntimeSessionKey,
        message: &str,
    ) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        let session_key = key.storage_key();
        reset_history(&session_key);
        let prompt = Self::doctor_domain_prompt(key, message);
        let text = run_zeroclaw_message(&prompt, &key.instance_id)
            .map(Self::normalize_doctor_output)
            .map_err(Self::map_error)?;
        append_history(&session_key, "system", &prompt);
        if let Some((invoke, note)) = Self::parse_tool_intent(&text) {
            append_history(&session_key, "assistant", &note);
            return Ok(vec![RuntimeEvent::chat_final(note), invoke]);
        }
        append_history(&session_key, "assistant", &text);
        Ok(vec![RuntimeEvent::chat_final(text)])
    }

    fn send(
        &self,
        key: &RuntimeSessionKey,
        message: &str,
    ) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        let session_key = key.storage_key();
        append_history(&session_key, "user", message);
        let prompt = build_prompt_with_history(&session_key, message);
        let guarded = Self::doctor_domain_prompt(key, &prompt);
        let text = run_zeroclaw_message(&guarded, &key.instance_id)
            .map(Self::normalize_doctor_output)
            .map_err(Self::map_error)?;
        if let Some((invoke, note)) = Self::parse_tool_intent(&text) {
            append_history(&session_key, "assistant", &note);
            return Ok(vec![RuntimeEvent::chat_final(note), invoke]);
        }
        append_history(&session_key, "assistant", &text);
        Ok(vec![RuntimeEvent::chat_final(text)])
    }
}

#[cfg(test)]
mod tests {
    use super::ZeroclawDoctorAdapter;

    #[test]
    fn parse_tool_intent_handles_mixed_text_with_embedded_json() {
        let raw = r#"好的，我来检查。
{"tool":"clawpal","args":"instance list","reason":"查看目录结构"}"#;
        let parsed = ZeroclawDoctorAdapter::parse_tool_intent(raw);
        assert!(parsed.is_some(), "should parse tool intent from mixed text");
    }

    #[test]
    fn parse_tool_intent_picks_tool_json_when_multiple_json_objects_exist() {
        let raw = r#"前置说明 {"step":"verify","reason":"ignore this"} 然后执行 {"tool":"clawpal","args":"health check --all","reason":"确认状态"}"#;
        let parsed = ZeroclawDoctorAdapter::parse_tool_intent(raw);
        assert!(
            parsed.is_some(),
            "should parse tool JSON even if another JSON appears first"
        );
    }
}
