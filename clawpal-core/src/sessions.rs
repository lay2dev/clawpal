use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionAnalysis {
    pub agent: String,
    pub session_id: String,
    pub file_path: String,
    pub size_bytes: u64,
    pub message_count: usize,
    pub user_message_count: usize,
    pub assistant_message_count: usize,
    pub last_activity: Option<String>,
    pub age_days: f64,
    pub total_tokens: u64,
    pub model: Option<String>,
    pub category: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionAnalysis {
    pub agent: String,
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub empty_count: usize,
    pub low_value_count: usize,
    pub valuable_count: usize,
    pub sessions: Vec<SessionAnalysis>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionFileInfo {
    pub path: String,
    pub relative_path: String,
    pub agent: String,
    pub kind: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionPreviewMessage {
    pub role: String,
    pub content: String,
}

pub type SessionPreview = Vec<SessionPreviewMessage>;

pub fn classify_session(
    size_bytes: u64,
    message_count: usize,
    user_message_count: usize,
    age_days: f64,
) -> &'static str {
    if size_bytes < 500 || message_count == 0 {
        "empty"
    } else if user_message_count <= 1 && age_days > 7.0 {
        "low_value"
    } else {
        "valuable"
    }
}

fn session_category_order(category: &str) -> u8 {
    match category {
        "empty" => 0,
        "low_value" => 1,
        _ => 2,
    }
}

pub fn sort_sessions(sessions: &mut [SessionAnalysis]) {
    sessions.sort_by(|a, b| {
        session_category_order(&a.category)
            .cmp(&session_category_order(&b.category))
            .then(
                b.age_days
                    .partial_cmp(&a.age_days)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
}

pub fn summarize_agent_sessions(
    agent: String,
    mut sessions: Vec<SessionAnalysis>,
) -> AgentSessionAnalysis {
    sort_sessions(&mut sessions);

    let total_files = sessions.len();
    let total_size_bytes = sessions.iter().map(|s| s.size_bytes).sum();
    let empty_count = sessions.iter().filter(|s| s.category == "empty").count();
    let low_value_count = sessions
        .iter()
        .filter(|s| s.category == "low_value")
        .count();
    let valuable_count = sessions.iter().filter(|s| s.category == "valuable").count();

    AgentSessionAnalysis {
        agent,
        total_files,
        total_size_bytes,
        empty_count,
        low_value_count,
        valuable_count,
        sessions,
    }
}

pub fn parse_session_analysis_entry(value: &Value) -> SessionAnalysis {
    let agent = value
        .get("agent")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let session_id = value
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let size_bytes = value.get("sizeBytes").and_then(Value::as_u64).unwrap_or(0);
    let message_count = value
        .get("messageCount")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let user_message_count = value
        .get("userMessageCount")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let assistant_message_count = value
        .get("assistantMessageCount")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let age_days = value.get("ageDays").and_then(Value::as_f64).unwrap_or(0.0);
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("sessions")
        .to_string();

    SessionAnalysis {
        agent,
        session_id,
        file_path: String::new(),
        size_bytes,
        message_count,
        user_message_count,
        assistant_message_count,
        last_activity: None,
        age_days,
        total_tokens: 0,
        model: None,
        category: classify_session(size_bytes, message_count, user_message_count, age_days)
            .to_string(),
        kind,
    }
}

pub fn parse_session_analysis_entry_line(line: &str) -> Result<Option<SessionAnalysis>, String> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(line)
        .map_err(|e| format!("Failed to parse remote session entry: {e}"))?;
    Ok(Some(parse_session_analysis_entry(&value)))
}

pub fn parse_session_analysis(raw: &str) -> Result<Vec<AgentSessionAnalysis>, String> {
    let parsed: Vec<Value> = serde_json::from_str(raw.trim()).map_err(|e| {
        format!(
            "Failed to parse remote session data: {e}; output: {}",
            &raw[..raw.len().min(500)]
        )
    })?;

    let mut agent_map: BTreeMap<String, Vec<SessionAnalysis>> = BTreeMap::new();

    for val in &parsed {
        let session = parse_session_analysis_entry(val);
        agent_map
            .entry(session.agent.clone())
            .or_default()
            .push(session);
    }

    let mut results = Vec::new();
    for (agent, sessions) in agent_map {
        results.push(summarize_agent_sessions(agent, sessions));
    }

    Ok(results)
}

pub fn filter_sessions_by_ids(json: &str, ids: &[&str]) -> Result<String, String> {
    let mut data = serde_json::from_str::<serde_json::Map<String, Value>>(json)
        .map_err(|e| format!("Failed to parse sessions json: {e}"))?;
    let id_set: HashSet<&str> = ids.iter().copied().collect();
    data.retain(|_key, val| {
        let sid = val.get("sessionId").and_then(Value::as_str).unwrap_or("");
        !id_set.contains(sid)
    });
    serde_json::to_string(&data).map_err(|e| format!("Failed to serialize sessions json: {e}"))
}

pub fn parse_session_file_list(raw: &str) -> Result<Vec<SessionFileInfo>, String> {
    let parsed: Vec<Value> = serde_json::from_str(raw.trim())
        .map_err(|e| format!("Failed to parse session file list: {e}"))?;
    Ok(parsed
        .iter()
        .map(|val| {
            let path = val
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            SessionFileInfo {
                relative_path: path.clone(),
                path,
                agent: val
                    .get("agent")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                kind: val
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("sessions")
                    .to_string(),
                size_bytes: val.get("sizeBytes").and_then(Value::as_u64).unwrap_or(0),
            }
        })
        .collect())
}

pub fn parse_session_preview_line(line: &str) -> Result<Option<SessionPreviewMessage>, String> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    let obj: Value = serde_json::from_str(line)
        .map_err(|e| format!("Failed to parse session preview line: {e}"))?;
    if obj.get("type").and_then(Value::as_str) != Some("message") {
        return Ok(None);
    }

    let role = obj
        .pointer("/message/role")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let content = obj
        .pointer("/message/content")
        .map(|c| {
            if let Some(arr) = c.as_array() {
                arr.iter()
                    .filter_map(|item| item.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else if let Some(s) = c.as_str() {
                s.to_string()
            } else {
                String::new()
            }
        })
        .unwrap_or_default();

    Ok(Some(SessionPreviewMessage { role, content }))
}

pub fn parse_session_preview(jsonl: &str) -> Result<SessionPreview, String> {
    let mut messages = Vec::new();
    for line in jsonl.lines() {
        if let Some(message) = parse_session_preview_line(line)? {
            messages.push(message);
        }
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_session_analysis_groups_and_classifies() {
        let raw = r#"[
          {"agent":"main","sessionId":"a","sizeBytes":100,"messageCount":0,"userMessageCount":0,"assistantMessageCount":0,"ageDays":1,"kind":"sessions"},
          {"agent":"main","sessionId":"b","sizeBytes":900,"messageCount":2,"userMessageCount":1,"assistantMessageCount":1,"ageDays":10,"kind":"sessions"}
        ]"#;
        let out = parse_session_analysis(raw).expect("parse");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].empty_count, 1);
        assert_eq!(out[0].low_value_count, 1);
    }

    #[test]
    fn filter_sessions_by_ids_removes_requested_ids() {
        let raw = r#"{"x":{"sessionId":"s1"},"y":{"sessionId":"s2"}}"#;
        let out = filter_sessions_by_ids(raw, &["s2"]).expect("filter");
        assert!(out.contains("s1"));
        assert!(!out.contains("s2"));
    }

    #[test]
    fn parse_session_file_list_returns_entries() {
        let raw = r#"[{"agent":"a","kind":"sessions","path":"a/sessions/1.jsonl","sizeBytes":42}]"#;
        let out = parse_session_file_list(raw).expect("parse");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].size_bytes, 42);
    }

    #[test]
    fn parse_session_preview_extracts_messages() {
        let raw = "{\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n";
        let out = parse_session_preview(raw).expect("preview");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
        assert_eq!(out[0].content, "hi");
    }

    #[test]
    fn parse_session_preview_handles_array_content() {
        let raw = r#"{"type":"message","message":{"role":"assistant","content":[{"text":"Hello"},{"text":" world"}]}}"#;
        let out = parse_session_preview(raw).expect("preview");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "assistant");
        assert_eq!(out[0].content, "Hello\n world");
    }

    #[test]
    fn parse_session_preview_skips_non_message_types() {
        let raw = "{\"type\":\"metadata\",\"data\":{}}\n{\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n";
        let out = parse_session_preview(raw).expect("preview");
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn parse_session_preview_skips_empty_lines() {
        let raw =
            "\n\n{\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":\"test\"}}\n\n";
        let out = parse_session_preview(raw).expect("preview");
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn parse_session_analysis_classifies_valuable() {
        let raw = r#"[
          {"agent":"main","sessionId":"v","sizeBytes":5000,"messageCount":10,"userMessageCount":5,"assistantMessageCount":5,"ageDays":1,"kind":"sessions"}
        ]"#;
        let out = parse_session_analysis(raw).expect("parse");
        assert_eq!(out[0].valuable_count, 1);
        assert_eq!(out[0].empty_count, 0);
        assert_eq!(out[0].low_value_count, 0);
    }

    #[test]
    fn parse_session_analysis_multiple_agents() {
        let raw = r#"[
          {"agent":"main","sessionId":"a","sizeBytes":100,"messageCount":0,"userMessageCount":0,"assistantMessageCount":0,"ageDays":1,"kind":"sessions"},
          {"agent":"cron","sessionId":"b","sizeBytes":5000,"messageCount":3,"userMessageCount":2,"assistantMessageCount":1,"ageDays":0.5,"kind":"sessions"}
        ]"#;
        let out = parse_session_analysis(raw).expect("parse");
        assert_eq!(out.len(), 2);
        let agents: Vec<&str> = out.iter().map(|a| a.agent.as_str()).collect();
        assert!(agents.contains(&"main"));
        assert!(agents.contains(&"cron"));
    }

    #[test]
    fn parse_session_analysis_empty_array() {
        let out = parse_session_analysis("[]").expect("parse");
        assert!(out.is_empty());
    }

    #[test]
    fn parse_session_analysis_sorts_by_category_then_age() {
        let raw = r#"[
          {"agent":"a","sessionId":"valuable","sizeBytes":5000,"messageCount":10,"userMessageCount":5,"assistantMessageCount":5,"ageDays":1,"kind":"sessions"},
          {"agent":"a","sessionId":"empty","sizeBytes":100,"messageCount":0,"userMessageCount":0,"assistantMessageCount":0,"ageDays":2,"kind":"sessions"}
        ]"#;
        let out = parse_session_analysis(raw).expect("parse");
        assert_eq!(out[0].sessions[0].category, "empty");
        assert_eq!(out[0].sessions[1].category, "valuable");
    }

    #[test]
    fn filter_sessions_by_ids_keeps_unmatched() {
        let raw = r#"{"a":{"sessionId":"s1"},"b":{"sessionId":"s2"},"c":{"sessionId":"s3"}}"#;
        let out = filter_sessions_by_ids(raw, &["s1", "s3"]).expect("filter");
        assert!(!out.contains("s1"));
        assert!(out.contains("s2"));
        assert!(!out.contains("s3"));
    }

    #[test]
    fn filter_sessions_by_ids_empty_filter() {
        let raw = r#"{"a":{"sessionId":"s1"}}"#;
        let out = filter_sessions_by_ids(raw, &[]).expect("filter");
        assert!(out.contains("s1"));
    }

    #[test]
    fn parse_session_file_list_empty() {
        let out = parse_session_file_list("[]").expect("parse");
        assert!(out.is_empty());
    }

    #[test]
    fn parse_session_file_list_multiple_entries() {
        let raw = r#"[
          {"agent":"a","kind":"sessions","path":"a/sessions/1.jsonl","sizeBytes":42},
          {"agent":"b","kind":"cron","path":"b/cron/2.jsonl","sizeBytes":100}
        ]"#;
        let out = parse_session_file_list(raw).expect("parse");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].agent, "a");
        assert_eq!(out[0].kind, "sessions");
        assert_eq!(out[1].agent, "b");
        assert_eq!(out[1].kind, "cron");
        assert_eq!(out[1].size_bytes, 100);
    }

    #[test]
    fn summarize_agent_sessions_counts_and_sorts() {
        let sessions = vec![
            SessionAnalysis {
                agent: "a".to_string(),
                session_id: "valuable".to_string(),
                file_path: String::new(),
                size_bytes: 2_000,
                message_count: 5,
                user_message_count: 3,
                assistant_message_count: 2,
                last_activity: None,
                age_days: 1.0,
                total_tokens: 0,
                model: None,
                category: "valuable".to_string(),
                kind: "sessions".to_string(),
            },
            SessionAnalysis {
                agent: "a".to_string(),
                session_id: "empty".to_string(),
                file_path: String::new(),
                size_bytes: 10,
                message_count: 0,
                user_message_count: 0,
                assistant_message_count: 0,
                last_activity: None,
                age_days: 9.0,
                total_tokens: 0,
                model: None,
                category: "empty".to_string(),
                kind: "sessions".to_string(),
            },
        ];

        let out = summarize_agent_sessions("a".to_string(), sessions);
        assert_eq!(out.total_files, 2);
        assert_eq!(out.empty_count, 1);
        assert_eq!(out.valuable_count, 1);
        assert_eq!(out.sessions[0].session_id, "empty");
    }

    #[test]
    fn parse_session_analysis_entry_line_handles_blank_lines() {
        let out = parse_session_analysis_entry_line("   ").expect("parse");
        assert!(out.is_none());
    }

    #[test]
    fn parse_session_preview_line_extracts_message() {
        let out = parse_session_preview_line(
            r#"{"type":"message","message":{"role":"assistant","content":"hello"}}"#,
        )
        .expect("parse");
        assert_eq!(
            out,
            Some(SessionPreviewMessage {
                role: "assistant".to_string(),
                content: "hello".to_string(),
            })
        );
    }

    #[test]
    fn parse_session_preview_line_skips_non_message_entries() {
        let out = parse_session_preview_line(r#"{"type":"metadata","foo":"bar"}"#).expect("parse");
        assert!(out.is_none());
    }
}
