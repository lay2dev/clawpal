use serde_json::Value;

pub type CronJob = Value;
pub type CronRun = Value;

pub fn parse_cron_jobs(json: &str) -> Result<Vec<CronJob>, String> {
    let parsed: Value = serde_json::from_str(json).unwrap_or(Value::Array(vec![]));
    let jobs = if let Some(arr) = parsed.pointer("/jobs") {
        arr.clone()
    } else {
        parsed
    };

    match jobs {
        Value::Array(arr) => Ok(arr
            .into_iter()
            .map(|mut v| {
                if let Value::Object(ref mut obj) = v {
                    if let Some(id) = obj.get("id").cloned() {
                        obj.entry("jobId".to_string()).or_insert(id);
                    }
                }
                v
            })
            .collect()),
        Value::Object(map) => Ok(map
            .into_iter()
            .map(|(k, mut v)| {
                if let Value::Object(ref mut obj) = v {
                    obj.entry("jobId".to_string())
                        .or_insert(Value::String(k.clone()));
                    obj.entry("id".to_string()).or_insert(Value::String(k));
                }
                v
            })
            .collect()),
        _ => Ok(vec![]),
    }
}

pub fn parse_cron_runs(jsonl: &str) -> Result<Vec<CronRun>, String> {
    let mut runs: Vec<Value> = jsonl
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str::<Value>(l)
                .map_err(|e| format!("Failed to parse cron run line: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    runs.reverse();
    Ok(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cron_jobs_supports_wrapper() {
        let raw = r#"{"version":1,"jobs":[{"id":"j1","expr":"* * * * *"}]}"#;
        let out = parse_cron_jobs(raw).expect("parse");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].get("jobId").and_then(Value::as_str), Some("j1"));
    }

    #[test]
    fn parse_cron_runs_parses_jsonl() {
        let raw = "{\"runId\":\"1\"}\n{\"runId\":\"2\"}\n";
        let out = parse_cron_runs(raw).expect("parse");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].get("runId").and_then(Value::as_str), Some("2"));
    }
}
