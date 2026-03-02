use serde_json::Value;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogStatus {
    pub alive: bool,
    pub deployed: bool,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

pub fn parse_watchdog_status(status_json: &str, ps_output: &str) -> WatchdogStatus {
    let alive = ps_output.trim() == "alive";
    let mut extra = match serde_json::from_str::<Value>(status_json) {
        Ok(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    extra.insert("alive".to_string(), Value::Bool(alive));
    let deployed = extra
        .get("deployed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    WatchdogStatus {
        alive,
        deployed,
        extra,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_watchdog_status_merges_alive_flag() {
        let out = parse_watchdog_status("{\"foo\":1}", "alive");
        assert!(out.alive);
        assert_eq!(out.extra.get("foo").and_then(Value::as_i64), Some(1));
        assert_eq!(out.extra.get("alive").and_then(Value::as_bool), Some(true));
    }
}
