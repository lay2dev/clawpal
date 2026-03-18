use super::*;

pub(crate) const DISCORD_REST_USER_AGENT: &str = "DiscordBot (https://openclaw.ai, 1.0)";

/// Fetch a Discord guild name via the Discord REST API using a bot token.
pub(crate) fn fetch_discord_guild_name(bot_token: &str, guild_id: &str) -> Result<String, String> {
    let url = format!("https://discord.com/api/v10/guilds/{guild_id}");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent(DISCORD_REST_USER_AGENT)
        .build()
        .map_err(|e| format!("Discord HTTP client error: {e}"))?;
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bot {bot_token}"))
        .send()
        .map_err(|e| format!("Discord API request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Discord API returned status {}", resp.status()));
    }
    let body: Value = resp
        .json()
        .map_err(|e| format!("Failed to parse Discord response: {e}"))?;
    body.get("name")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| "No name field in Discord guild response".to_string())
}

/// Fetch Discord channels for a guild via REST API using a bot token.
pub(crate) fn fetch_discord_guild_channels(
    bot_token: &str,
    guild_id: &str,
) -> Result<Vec<(String, String)>, String> {
    let url = format!("https://discord.com/api/v10/guilds/{guild_id}/channels");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent(DISCORD_REST_USER_AGENT)
        .build()
        .map_err(|e| format!("Discord HTTP client error: {e}"))?;
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bot {bot_token}"))
        .send()
        .map_err(|e| format!("Discord API request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Discord API returned status {}", resp.status()));
    }
    let body: Value = resp
        .json()
        .map_err(|e| format!("Failed to parse Discord response: {e}"))?;
    let arr = body
        .as_array()
        .ok_or_else(|| "Discord response is not an array".to_string())?;
    let mut out = Vec::new();
    for item in arr {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        // Filter out categories (type 4), voice channels (type 2), and stage channels (type 13)
        let channel_type = item.get("type").and_then(Value::as_u64).unwrap_or(0);
        if channel_type == 4 || channel_type == 2 || channel_type == 13 {
            continue;
        }
        if let (Some(id), Some(name)) = (id, name) {
            if !out.iter().any(|(existing_id, _)| *existing_id == id) {
                out.push((id, name));
            }
        }
    }
    Ok(out)
}

/// Parse `openclaw channels resolve --json` output into a map of id -> name.
pub(crate) fn parse_resolve_name_map(stdout: &str) -> Option<HashMap<String, String>> {
    let json_str = extract_last_json_array(stdout)?;
    let parsed: Vec<Value> = serde_json::from_str(json_str).ok()?;
    let mut map = HashMap::new();
    for item in parsed {
        let resolved = item
            .get("resolved")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !resolved {
            continue;
        }
        if let (Some(input), Some(name)) = (
            item.get("input").and_then(Value::as_str),
            item.get("name").and_then(Value::as_str),
        ) {
            let name = name.trim().to_string();
            if !name.is_empty() {
                map.insert(input.to_string(), name);
            }
        }
    }
    Some(map)
}

/// Parse `openclaw directory groups list --json` output into channel ids.
pub(crate) fn parse_directory_group_channel_ids(stdout: &str) -> Vec<String> {
    let json_str = match extract_last_json_array(stdout) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let parsed: Vec<Value> = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut ids = Vec::new();
    for item in parsed {
        let raw = item.get("id").and_then(Value::as_str).unwrap_or("");
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = trimmed
            .strip_prefix("channel:")
            .unwrap_or(trimmed)
            .trim()
            .to_string();
        if normalized.is_empty() || ids.contains(&normalized) {
            continue;
        }
        ids.push(normalized);
    }
    ids
}

pub(crate) fn collect_discord_config_guild_ids(discord_cfg: Option<&Value>) -> Vec<String> {
    let mut guild_ids = Vec::new();
    if let Some(guilds) = discord_cfg
        .and_then(|d| d.get("guilds"))
        .and_then(Value::as_object)
    {
        for guild_id in guilds.keys() {
            if !guild_ids.contains(guild_id) {
                guild_ids.push(guild_id.clone());
            }
        }
    }
    if let Some(accounts) = discord_cfg
        .and_then(|d| d.get("accounts"))
        .and_then(Value::as_object)
    {
        for account in accounts.values() {
            if let Some(guilds) = account.get("guilds").and_then(Value::as_object) {
                for guild_id in guilds.keys() {
                    if !guild_ids.contains(guild_id) {
                        guild_ids.push(guild_id.clone());
                    }
                }
            }
        }
    }
    guild_ids
}

pub(crate) fn collect_discord_config_guild_name_fallbacks(
    discord_cfg: Option<&Value>,
) -> HashMap<String, String> {
    let mut guild_names = HashMap::new();

    if let Some(guilds) = discord_cfg
        .and_then(|d| d.get("guilds"))
        .and_then(Value::as_object)
    {
        for (guild_id, guild_val) in guilds {
            let guild_name = guild_val
                .get("slug")
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if let Some(name) = guild_name {
                guild_names.entry(guild_id.clone()).or_insert(name);
            }
        }
    }

    if let Some(accounts) = discord_cfg
        .and_then(|d| d.get("accounts"))
        .and_then(Value::as_object)
    {
        for account in accounts.values() {
            if let Some(guilds) = account.get("guilds").and_then(Value::as_object) {
                for (guild_id, guild_val) in guilds {
                    let guild_name = guild_val
                        .get("slug")
                        .and_then(Value::as_str)
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    if let Some(name) = guild_name {
                        guild_names.entry(guild_id.clone()).or_insert(name);
                    }
                }
            }
        }
    }

    guild_names
}

pub(crate) fn collect_discord_cache_guild_name_fallbacks(
    entries: &[DiscordGuildChannel],
) -> HashMap<String, String> {
    let mut guild_names = HashMap::new();
    for entry in entries {
        let name = entry.guild_name.trim();
        if name.is_empty() || name == entry.guild_id {
            continue;
        }
        guild_names
            .entry(entry.guild_id.clone())
            .or_insert_with(|| name.to_string());
    }
    guild_names
}

pub(crate) fn parse_discord_cache_guild_name_fallbacks(cache_json: &str) -> HashMap<String, String> {
    let entries: Vec<DiscordGuildChannel> = serde_json::from_str(cache_json).unwrap_or_default();
    collect_discord_cache_guild_name_fallbacks(&entries)
}

#[cfg(test)]
mod discord_directory_parse_tests {
    use super::{
        parse_directory_group_channel_ids, parse_discord_cache_guild_name_fallbacks,
        DiscordGuildChannel,
    };

    #[test]
    fn parse_directory_groups_extracts_channel_ids() {
        let stdout = r#"
[plugins] example
[
  {"kind":"group","id":"channel:123"},
  {"kind":"group","id":"channel:456"},
  {"kind":"group","id":"channel:123"},
  {"kind":"group","id":"  channel:789  "}
]
"#;
        let ids = parse_directory_group_channel_ids(stdout);
        assert_eq!(ids, vec!["123", "456", "789"]);
    }

    #[test]
    fn parse_directory_groups_handles_missing_json() {
        let stdout = "not json";
        let ids = parse_directory_group_channel_ids(stdout);
        assert!(ids.is_empty());
    }

    #[test]
    fn parse_discord_cache_guild_name_fallbacks_uses_non_id_names() {
        let payload = vec![
            DiscordGuildChannel {
                guild_id: "1".into(),
                guild_name: "Guild One".into(),
                channel_id: "11".into(),
                channel_name: "chan-1".into(),
                default_agent_id: None,
            },
            DiscordGuildChannel {
                guild_id: "1".into(),
                guild_name: "1".into(),
                channel_id: "12".into(),
                channel_name: "chan-2".into(),
                default_agent_id: None,
            },
            DiscordGuildChannel {
                guild_id: "2".into(),
                guild_name: "2".into(),
                channel_id: "21".into(),
                channel_name: "chan-3".into(),
                default_agent_id: None,
            },
        ];
        let text = serde_json::to_string(&payload).expect("serialize payload");
        let fallbacks = parse_discord_cache_guild_name_fallbacks(&text);
        assert_eq!(fallbacks.get("1"), Some(&"Guild One".to_string()));
        assert!(!fallbacks.contains_key("2"));
    }
}
