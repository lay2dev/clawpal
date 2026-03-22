use super::*;

const DISCORD_CACHE_TTL_SECS: u64 = 7 * 24 * 3600; // 1 week

fn unix_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn extract_discord_bot_token(discord_cfg: Option<&Value>) -> Option<String> {
    discord_cfg
        .and_then(|d| d.get("botToken").or_else(|| d.get("token")))
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .or_else(|| {
            discord_cfg
                .and_then(|d| d.get("accounts"))
                .and_then(Value::as_object)
                .and_then(|accounts| {
                    accounts.values().find_map(|acct| {
                        acct.get("token")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                    })
                })
        })
}

fn summarize_resolution_error(stderr: &str, stdout: &str) -> String {
    let combined = format!("{} {}", stderr.trim(), stdout.trim())
        .trim()
        .replace('\n', " ");
    if combined.is_empty() {
        "unknown error".to_string()
    } else {
        combined
    }
}

fn append_resolution_warning(target: &mut Option<String>, message: &str) {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return;
    }
    match target {
        Some(existing) => {
            if !existing.contains(trimmed) {
                existing.push(' ');
                existing.push_str(trimmed);
            }
        }
        None => *target = Some(trimmed.to_string()),
    }
}

fn discord_sections_from_openclaw_config(cfg: &Value) -> (Value, Value) {
    let discord_section = cfg
        .pointer("/channels/discord")
        .cloned()
        .unwrap_or(Value::Null);
    let bindings_section = cfg
        .get("bindings")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    (discord_section, bindings_section)
}

fn agent_overviews_from_openclaw_config(
    cfg: &Value,
    online_set: &std::collections::HashSet<String>,
) -> Vec<AgentOverview> {
    let mut agents = collect_agent_overviews_from_config(cfg);
    for agent in &mut agents {
        agent.online = online_set.contains(&agent.id);
    }
    agents
}

#[tauri::command]
pub async fn remote_list_discord_guild_channels(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    force_refresh: bool,
) -> Result<Vec<DiscordGuildChannel>, String> {
    // TTL gate: if the discord-guild-channels.json is fresh and not forced,
    // return the cached file immediately without any SSH commands.
    if !force_refresh {
        let meta_text = pool
            .sftp_read(&host_id, "~/.clawpal/discord-channels-meta.json")
            .await
            .unwrap_or_default();
        if let Ok(meta) = serde_json::from_str::<Value>(&meta_text) {
            if let Some(cached_at) = meta.get("cachedAt").and_then(Value::as_u64) {
                if unix_now_secs().saturating_sub(cached_at) < DISCORD_CACHE_TTL_SECS {
                    let cache_text = pool
                        .sftp_read(&host_id, "~/.clawpal/discord-guild-channels.json")
                        .await
                        .unwrap_or_default();
                    let entries: Vec<DiscordGuildChannel> =
                        serde_json::from_str(&cache_text).unwrap_or_default();
                    if !entries.is_empty() {
                        return Ok(entries);
                    }
                }
            }
        }
    }

    let output = crate::cli_runner::run_openclaw_remote(
        &pool,
        &host_id,
        &["config", "get", "channels.discord", "--json"],
    )
    .await?;
    let config_command_warning = if output.exit_code == 0 {
        None
    } else {
        Some(format!(
            "Discord config lookup failed: {}",
            summarize_resolution_error(&output.stderr, &output.stdout)
        ))
    };
    let bindings_output = crate::cli_runner::run_openclaw_remote(
        &pool,
        &host_id,
        &["config", "get", "bindings", "--json"],
    )
    .await?;
    let cli_discord = if output.exit_code == 0 {
        crate::cli_runner::parse_json_output(&output).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    // The openclaw CLI schema validator may strip 'guilds'/'botToken' from the
    // discord section even on exit_code 0.  Fall back to raw SFTP config read
    // whenever the CLI output lacks guilds/accounts so we don't miss channels.
    let cli_has_discord =
        cli_discord.get("guilds").is_some() || cli_discord.get("accounts").is_some();
    let config_fallback =
        if cli_has_discord && output.exit_code == 0 && bindings_output.exit_code == 0 {
            None
        } else {
            remote_read_openclaw_config_text_and_json(&pool, &host_id)
                .await
                .ok()
                .map(|(_, _, cfg)| cfg)
        };
    let (fallback_discord_section, fallback_bindings_section) = config_fallback
        .as_ref()
        .map(discord_sections_from_openclaw_config)
        .unwrap_or_else(|| (Value::Null, Value::Array(Vec::new())));
    let discord_section = if cli_has_discord {
        cli_discord
    } else {
        fallback_discord_section
    };
    let bindings_section = if bindings_output.exit_code == 0 {
        crate::cli_runner::parse_json_output(&bindings_output).unwrap_or(fallback_bindings_section)
    } else {
        fallback_bindings_section
    };
    // Wrap to match existing code expectations (rest of function uses cfg.get("channels").and_then(|c| c.get("discord")))
    let cfg = serde_json::json!({
        "channels": { "discord": discord_section },
        "bindings": bindings_section
    });

    let discord_cfg = cfg.get("channels").and_then(|c| c.get("discord"));
    let configured_single_guild_id = discord_cfg
        .and_then(|d| d.get("guilds"))
        .and_then(Value::as_object)
        .and_then(|guilds| {
            if guilds.len() == 1 {
                guilds.keys().next().cloned()
            } else {
                None
            }
        });

    // Extract bot token: top-level first, then fall back to first account token
    let bot_token = discord_cfg
        .and_then(|d| d.get("botToken").or_else(|| d.get("token")))
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .or_else(|| {
            discord_cfg
                .and_then(|d| d.get("accounts"))
                .and_then(Value::as_object)
                .and_then(|accounts| {
                    accounts.values().find_map(|acct| {
                        acct.get("token")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                    })
                })
        });
    let existing_cache_text = pool
        .sftp_read(&host_id, "~/.clawpal/discord-guild-channels.json")
        .await
        .unwrap_or_default();
    let mut guild_name_fallback_map =
        parse_discord_cache_guild_name_fallbacks(&existing_cache_text);
    guild_name_fallback_map.extend(collect_discord_config_guild_name_fallbacks(discord_cfg));
    // Also build a channel name fallback from the existing cache so that if CLI
    // resolve fails we don't overwrite previously-resolved names with raw IDs.
    let channel_name_fallback_map: HashMap<String, String> = {
        let cached: Vec<DiscordGuildChannel> =
            serde_json::from_str(&existing_cache_text).unwrap_or_default();
        cached
            .into_iter()
            .filter(|e| e.channel_name != e.channel_id)
            .map(|e| (e.channel_id, e.channel_name))
            .collect()
    };

    // Load the id→name cache so we can skip Discord REST calls for entries
    // that were successfully resolved recently.
    let id_cache_text = pool
        .sftp_read(&host_id, "~/.clawpal/discord-id-cache.json")
        .await
        .unwrap_or_default();
    let mut id_cache = DiscordIdCache::from_str(&id_cache_text);
    let now_secs = unix_now_secs();

    let core_channels = clawpal_core::discovery::parse_guild_channels(&cfg.to_string())?;
    let mut entries: Vec<DiscordGuildChannel> = core_channels
        .iter()
        .map(|c| DiscordGuildChannel {
            guild_id: c.guild_id.clone(),
            guild_name: c.guild_name.clone(),
            channel_id: c.channel_id.clone(),
            channel_name: c.channel_name.clone(),
            default_agent_id: None,
            resolution_warning: None,
        })
        .collect();
    let mut channel_ids: Vec<String> = entries.iter().map(|e| e.channel_id.clone()).collect();
    let mut unresolved_guild_ids: Vec<String> = entries
        .iter()
        .filter(|e| e.guild_name == e.guild_id)
        .map(|e| e.guild_id.clone())
        .collect();
    unresolved_guild_ids.sort();
    unresolved_guild_ids.dedup();
    let mut channel_warning_by_id: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut shared_channel_warning: Option<String> = None;
    let mut shared_guild_warning: Option<String> = None;

    // Fallback A: if we have token + guild ids, fetch channels from Discord REST directly.
    // This avoids hard-failing when CLI rejects config due non-critical schema drift.
    if channel_ids.is_empty() {
        let configured_guild_ids = collect_discord_config_guild_ids(discord_cfg);
        if let Some(token) = bot_token.clone() {
            let rest_entries = tokio::task::spawn_blocking(move || {
                let mut out: Vec<DiscordGuildChannel> = Vec::new();
                for guild_id in configured_guild_ids {
                    if let Ok(channels) = fetch_discord_guild_channels(&token, &guild_id) {
                        for (channel_id, channel_name) in channels {
                            if out
                                .iter()
                                .any(|e| e.guild_id == guild_id && e.channel_id == channel_id)
                            {
                                continue;
                            }
                            out.push(DiscordGuildChannel {
                                guild_id: guild_id.clone(),
                                guild_name: guild_id.clone(),
                                channel_id,
                                channel_name,
                                default_agent_id: None,
                                resolution_warning: None,
                            });
                        }
                    }
                }
                out
            })
            .await
            .unwrap_or_default();
            for entry in rest_entries {
                if entries
                    .iter()
                    .any(|e| e.guild_id == entry.guild_id && e.channel_id == entry.channel_id)
                {
                    continue;
                }
                channel_ids.push(entry.channel_id.clone());
                entries.push(entry);
            }
        }
    }

    // Fallback B: query channel ids from directory and keep compatibility
    // with existing cache shape when config has no explicit channel map.
    if channel_ids.is_empty() {
        let cmd = "openclaw directory groups list --channel discord --json";
        if let Ok(r) = pool.exec_login(&host_id, cmd).await {
            if r.exit_code == 0 && !r.stdout.trim().is_empty() {
                for channel_id in parse_directory_group_channel_ids(&r.stdout) {
                    if entries.iter().any(|e| e.channel_id == channel_id) {
                        continue;
                    }
                    let (guild_id, guild_name) =
                        if let Some(gid) = configured_single_guild_id.clone() {
                            (gid.clone(), gid)
                        } else {
                            ("discord".to_string(), "Discord".to_string())
                        };
                    channel_ids.push(channel_id.clone());
                    entries.push(DiscordGuildChannel {
                        guild_id,
                        guild_name,
                        channel_id: channel_id.clone(),
                        channel_name: channel_id,
                        default_agent_id: None,
                        resolution_warning: None,
                    });
                }
            } else if r.exit_code != 0 {
                shared_channel_warning = Some(format!(
                    "Discord directory lookup failed: {}",
                    summarize_resolution_error(&r.stderr, &r.stdout)
                ));
            }
        }
    }

    // Resolve channel names: apply id cache first, then call CLI for misses.
    {
        // Apply cached channel names immediately.
        for entry in &mut entries {
            if entry.channel_name == entry.channel_id {
                if let Some(name) =
                    id_cache.get_channel_name(&entry.channel_id, now_secs, force_refresh)
                {
                    entry.channel_name = name.to_string();
                }
            }
        }
        // Collect IDs that still need CLI resolution.
        let uncached_ids: Vec<String> = channel_ids
            .iter()
            .filter(|id| {
                id_cache
                    .get_channel_name(id, now_secs, force_refresh)
                    .is_none()
            })
            .cloned()
            .collect();
        if !uncached_ids.is_empty() {
            let ids_arg = uncached_ids.join(" ");
            let cmd = format!(
                "openclaw channels resolve --json --channel discord --kind auto {}",
                ids_arg
            );
            if let Ok(r) = pool.exec_login(&host_id, &cmd).await {
                if r.exit_code == 0 && !r.stdout.trim().is_empty() {
                    if let Some(name_map) = parse_resolve_name_map(&r.stdout) {
                        for entry in &mut entries {
                            if let Some(name) = name_map.get(&entry.channel_id) {
                                entry.channel_name = name.clone();
                                id_cache.put_channel(
                                    entry.channel_id.clone(),
                                    name.clone(),
                                    now_secs,
                                );
                            }
                        }
                    }
                } else {
                    // Batch failed (e.g. one channel 404). Fall back to resolving one-by-one
                    // so a single bad channel doesn't block the rest.
                    shared_channel_warning = Some(format!(
                        "Discord channel name lookup failed: {}",
                        summarize_resolution_error(&r.stderr, &r.stdout)
                    ));
                    eprintln!("[discord] channels resolve batch failed exit={} stderr={:?}, trying one-by-one",
                        r.exit_code, r.stderr.trim());
                    for channel_id in &uncached_ids {
                        let single_cmd = format!(
                            "openclaw channels resolve --json --channel discord --kind auto {}",
                            channel_id
                        );
                        if let Ok(sr) = pool.exec_login(&host_id, &single_cmd).await {
                            if sr.exit_code == 0 {
                                if let Some(name_map) = parse_resolve_name_map(&sr.stdout) {
                                    for entry in &mut entries {
                                        if entry.channel_id == *channel_id {
                                            if let Some(name) = name_map.get(channel_id) {
                                                entry.channel_name = name.clone();
                                                id_cache.put_channel(
                                                    channel_id.clone(),
                                                    name.clone(),
                                                    now_secs,
                                                );
                                            }
                                        }
                                    }
                                }
                            } else {
                                channel_warning_by_id.insert(
                                    channel_id.clone(),
                                    format!(
                                        "Discord channel name lookup failed: {}",
                                        summarize_resolution_error(&sr.stderr, &sr.stdout)
                                    ),
                                );
                                eprintln!(
                                    "[discord] channels resolve single {} exit={} stderr={:?}",
                                    channel_id,
                                    sr.exit_code,
                                    sr.stderr.trim()
                                );
                            }
                        }
                    }
                }
            }
        }
        // Fallback: for entries still unresolved, use names from the previous cache.
        for entry in &mut entries {
            if entry.channel_name == entry.channel_id {
                if let Some(name) = channel_name_fallback_map.get(&entry.channel_id) {
                    entry.channel_name = name.clone();
                }
            }
        }
    }

    // Resolve guild names via Discord REST API, using id cache to skip known guilds.
    {
        let unresolved: Vec<String> = entries
            .iter()
            .filter(|e| e.guild_name == e.guild_id)
            .map(|e| e.guild_id.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Apply already-cached names.
        for entry in &mut entries {
            if entry.guild_name == entry.guild_id {
                if let Some(name) =
                    id_cache.get_guild_name(&entry.guild_id, now_secs, force_refresh)
                {
                    entry.guild_name = name.to_string();
                }
            }
        }

        // Fetch from Discord REST for guilds still unresolved after cache check.
        let needs_rest: Vec<String> = unresolved
            .into_iter()
            .filter(|gid| {
                id_cache
                    .get_guild_name(gid, now_secs, force_refresh)
                    .is_none()
            })
            .collect();

        if let Some(token) = bot_token {
            if !needs_rest.is_empty() {
                let guild_name_map = tokio::task::spawn_blocking(move || {
                    let mut map = std::collections::HashMap::new();
                    for gid in &needs_rest {
                        if let Ok(name) = fetch_discord_guild_name(&token, gid) {
                            map.insert(gid.clone(), name);
                        }
                    }
                    map
                })
                .await
                .unwrap_or_default();
                for (gid, name) in &guild_name_map {
                    id_cache.put_guild(gid.clone(), name.clone(), now_secs);
                }
                for entry in &mut entries {
                    if let Some(name) = guild_name_map.get(&entry.guild_id) {
                        entry.guild_name = name.clone();
                    }
                }
            }
        } else if !needs_rest.is_empty() {
            shared_guild_warning = Some(
                "Discord guild name lookup skipped because no Discord bot token is configured."
                    .to_string(),
            );
        }
    }

    // Config-derived slug/name fallbacks (last resort for guilds still showing as IDs).
    for entry in &mut entries {
        if entry.guild_name == entry.guild_id {
            if let Some(name) = guild_name_fallback_map.get(&entry.guild_id) {
                entry.guild_name = name.clone();
            }
        }
    }

    for entry in &mut entries {
        entry.resolution_warning = None;
        if entry.channel_name == entry.channel_id {
            if let Some(message) = channel_warning_by_id.get(&entry.channel_id) {
                append_resolution_warning(&mut entry.resolution_warning, message);
            } else if let Some(message) = shared_channel_warning.as_deref() {
                append_resolution_warning(&mut entry.resolution_warning, message);
            } else if let Some(message) = config_command_warning.as_deref() {
                append_resolution_warning(&mut entry.resolution_warning, message);
            } else {
                append_resolution_warning(
                    &mut entry.resolution_warning,
                    "Discord channel name is still unresolved after fallback to cached data.",
                );
            }
        }
        if entry.guild_name == entry.guild_id {
            if let Some(message) = shared_guild_warning.as_deref() {
                append_resolution_warning(&mut entry.resolution_warning, message);
            } else if let Some(message) = config_command_warning.as_deref() {
                append_resolution_warning(&mut entry.resolution_warning, message);
            } else {
                append_resolution_warning(
                    &mut entry.resolution_warning,
                    "Discord guild name is still unresolved after fallback to cached data.",
                );
            }
        }
    }

    // Resolve default agent per guild from account config + bindings (remote)
    {
        // Build account_id -> default agent_id from bindings (account-level, no peer)
        let mut account_agent_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        if let Some(bindings) = cfg.get("bindings").and_then(Value::as_array) {
            for b in bindings {
                let m = match b.get("match") {
                    Some(m) => m,
                    None => continue,
                };
                if m.get("channel").and_then(Value::as_str) != Some("discord") {
                    continue;
                }
                let account_id = match m.get("accountId").and_then(Value::as_str) {
                    Some(s) => s,
                    None => continue,
                };
                if m.get("peer").and_then(|p| p.get("id")).is_some() {
                    continue;
                } // skip channel-specific
                if let Some(agent_id) = b.get("agentId").and_then(Value::as_str) {
                    account_agent_map
                        .entry(account_id.to_string())
                        .or_insert_with(|| agent_id.to_string());
                }
            }
        }
        // Build guild_id -> default agent from account->guild mapping
        let mut guild_default_agent: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        if let Some(accounts) = discord_cfg
            .and_then(|d| d.get("accounts"))
            .and_then(Value::as_object)
        {
            for (account_id, account_val) in accounts {
                let agent = account_agent_map
                    .get(account_id)
                    .cloned()
                    .unwrap_or_else(|| account_id.clone());
                if let Some(guilds) = account_val.get("guilds").and_then(Value::as_object) {
                    for guild_id in guilds.keys() {
                        guild_default_agent
                            .entry(guild_id.clone())
                            .or_insert(agent.clone());
                    }
                }
            }
        }
        for entry in &mut entries {
            if entry.default_agent_id.is_none() {
                if let Some(agent_id) = guild_default_agent.get(&entry.guild_id) {
                    entry.default_agent_id = Some(agent_id.clone());
                }
            }
        }
    }

    // Persist to remote cache + write metadata for TTL gate + id cache
    if !entries.is_empty() {
        let json = serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())?;
        let _ = pool
            .sftp_write(&host_id, "~/.clawpal/discord-guild-channels.json", &json)
            .await;
        let meta = serde_json::json!({ "cachedAt": unix_now_secs() }).to_string();
        let _ = pool
            .sftp_write(&host_id, "~/.clawpal/discord-channels-meta.json", &meta)
            .await;
        let id_cache_json = id_cache.to_json();
        let _ = pool
            .sftp_write(&host_id, "~/.clawpal/discord-id-cache.json", &id_cache_json)
            .await;
    }

    Ok(entries)
}

pub async fn remote_list_bindings_with_pool(
    pool: &SshConnectionPool,
    host_id: String,
) -> Result<Vec<Value>, String> {
    let output = crate::cli_runner::run_openclaw_remote(
        pool,
        &host_id,
        &["config", "get", "bindings", "--json"],
    )
    .await?;
    // "bindings" may not exist yet — treat non-zero exit with "not found" as empty
    if output.exit_code != 0 {
        let msg = format!("{} {}", output.stderr, output.stdout).to_lowercase();
        if msg.contains("not found") {
            return Ok(Vec::new());
        }
    }
    let json = crate::cli_runner::parse_json_output(&output)?;
    clawpal_core::discovery::parse_bindings(&json.to_string())
}

#[tauri::command]
pub async fn remote_list_bindings(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<Value>, String> {
    remote_list_bindings_with_pool(pool.inner(), host_id).await
}

#[tauri::command]
pub async fn remote_list_channels_minimal(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<ChannelNode>, String> {
    let output = crate::cli_runner::run_openclaw_remote(
        &pool,
        &host_id,
        &["config", "get", "channels", "--json"],
    )
    .await?;
    // channels key might not exist yet
    if output.exit_code != 0 {
        let msg = format!("{} {}", output.stderr, output.stdout).to_lowercase();
        if msg.contains("not found") {
            return Ok(Vec::new());
        }
        return Err(format!(
            "openclaw config get channels failed: {}",
            output.stderr
        ));
    }
    let channels_val = crate::cli_runner::parse_json_output(&output).unwrap_or(Value::Null);
    // Wrap in top-level object with "channels" key so collect_channel_nodes works
    let cfg = serde_json::json!({ "channels": channels_val });
    Ok(collect_channel_nodes(&cfg))
}

pub async fn remote_list_agents_overview_with_pool(
    pool: &SshConnectionPool,
    host_id: String,
) -> Result<Vec<AgentOverview>, String> {
    let output =
        crate::cli_runner::run_openclaw_remote(pool, &host_id, &["agents", "list", "--json"])
            .await?;
    // Check which agents have sessions remotely (single command, batch check)
    // Lists agents whose sessions.json is larger than 2 bytes (not just "{}")
    let online_set = match pool.exec_login(
        &host_id,
        "for d in ~/.openclaw/agents/*/sessions/sessions.json; do [ -f \"$d\" ] && [ $(wc -c < \"$d\") -gt 2 ] && basename $(dirname $(dirname \"$d\")); done",
    ).await {
        Ok(result) => {
            result.stdout.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect::<std::collections::HashSet<String>>()
        }
        Err(_) => std::collections::HashSet::new(), // fallback: all offline
    };
    if output.exit_code != 0 {
        let details = format!("{}\n{}", output.stderr.trim(), output.stdout.trim());
        if clawpal_core::doctor::owner_display_parse_error(&details) {
            crate::commands::logs::log_remote_autofix_suppressed(
                &host_id,
                "openclaw agents list --json",
                "owner_display_parse_error",
            );
        }
        if let Ok((_, _, cfg)) = remote_read_openclaw_config_text_and_json(pool, &host_id).await {
            return Ok(agent_overviews_from_openclaw_config(&cfg, &online_set));
        }
        return Err(format!(
            "openclaw agents list failed ({}): {}",
            output.exit_code,
            details.trim()
        ));
    }
    let json = crate::cli_runner::parse_json_output(&output)?;
    parse_agents_cli_output(&json, Some(&online_set))
}

#[tauri::command]
pub async fn remote_list_agents_overview(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<AgentOverview>, String> {
    remote_list_agents_overview_with_pool(pool.inner(), host_id).await
}

#[tauri::command]
pub async fn list_channels() -> Result<Vec<ChannelNode>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let paths = resolve_paths();
        let cfg = read_openclaw_config(&paths)?;
        let mut nodes = collect_channel_nodes(&cfg);
        enrich_channel_display_names(&paths, &cfg, &mut nodes)?;
        Ok(nodes)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_channels_minimal(
    cache: tauri::State<'_, crate::cli_runner::CliCache>,
) -> Result<Vec<ChannelNode>, String> {
    let cache_key = local_cli_cache_key("channels-minimal");
    let ttl = Some(std::time::Duration::from_secs(30));
    if let Some(cached) = cache.get(&cache_key, ttl) {
        return serde_json::from_str(&cached).map_err(|e| e.to_string());
    }
    let cache = cache.inner().clone();
    let cache_key_cloned = cache_key.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let output = crate::cli_runner::run_openclaw(&["config", "get", "channels", "--json"])
            .map_err(|e| format!("Failed to run openclaw: {e}"))?;
        if output.exit_code != 0 {
            let msg = format!("{} {}", output.stderr, output.stdout).to_lowercase();
            if msg.contains("not found") {
                return Ok(Vec::new());
            }
            // Fallback: direct read
            let paths = resolve_paths();
            let cfg = read_openclaw_config(&paths)?;
            let result = collect_channel_nodes(&cfg);
            if let Ok(serialized) = serde_json::to_string(&result) {
                cache.set(cache_key_cloned, serialized);
            }
            return Ok(result);
        }
        let channels_val = crate::cli_runner::parse_json_output(&output).unwrap_or(Value::Null);
        let cfg = serde_json::json!({ "channels": channels_val });
        let result = collect_channel_nodes(&cfg);
        if let Ok(serialized) = serde_json::to_string(&result) {
            cache.set(cache_key_cloned, serialized);
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn list_discord_guild_channels() -> Result<Vec<DiscordGuildChannel>, String> {
    let paths = resolve_paths();
    let cache_file = paths.clawpal_dir.join("discord-guild-channels.json");
    if cache_file.exists() {
        let text = fs::read_to_string(&cache_file).map_err(|e| e.to_string())?;
        let entries: Vec<DiscordGuildChannel> = serde_json::from_str(&text).unwrap_or_default();
        return Ok(entries);
    }
    Ok(Vec::new())
}

/// Fast path: return guild channels from disk cache merged with config-derived
/// structure.  Never calls Discord REST or CLI subprocesses, so it completes in
/// < 50 ms locally.  Unresolved names are left as raw IDs — the caller is
/// expected to trigger a full `refresh_discord_guild_channels` in the background
/// to enrich them.
#[tauri::command]
pub async fn list_discord_guild_channels_fast() -> Result<Vec<DiscordGuildChannel>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = resolve_paths();
        // Layer 0: read existing cache (may contain resolved names from a prior refresh)
        let cache_file = paths.clawpal_dir.join("discord-guild-channels.json");
        let cached: Vec<DiscordGuildChannel> = if cache_file.exists() {
            fs::read_to_string(&cache_file)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        // Layer 1: parse config to discover any guild/channel pairs not yet in the cache
        let cfg = match read_openclaw_config(&paths) {
            Ok(c) => c,
            Err(_) => return Ok(cached), // config unreadable — return cache-only
        };
        let core_channels =
            clawpal_core::discovery::parse_guild_channels(&cfg.to_string()).unwrap_or_default();

        // Build a lookup from cached entries so we can reuse resolved names
        let mut cache_map: std::collections::HashMap<(String, String), DiscordGuildChannel> =
            cached
                .into_iter()
                .map(|e| ((e.guild_id.clone(), e.channel_id.clone()), e))
                .collect();

        let mut result: Vec<DiscordGuildChannel> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for ch in &core_channels {
            let key = (ch.guild_id.clone(), ch.channel_id.clone());
            if !seen.insert(key.clone()) {
                continue;
            }
            if let Some(cached_entry) = cache_map.remove(&key) {
                // Prefer cached entry — it has resolved names from the last full refresh
                result.push(cached_entry);
            } else {
                result.push(DiscordGuildChannel {
                    guild_id: ch.guild_id.clone(),
                    guild_name: ch.guild_name.clone(),
                    channel_id: ch.channel_id.clone(),
                    channel_name: ch.channel_name.clone(),
                    default_agent_id: None,
                    resolution_warning: None,
                });
            }
        }

        // Append any cached entries not in config (e.g. from bindings or directory discovery)
        for (key, entry) in cache_map {
            if seen.insert(key) {
                result.push(entry);
            }
        }

        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Fast path for remote instances: read config-derived guild channels without
/// calling Discord REST or remote CLI resolve.
#[tauri::command]
pub async fn remote_list_discord_guild_channels_fast(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<DiscordGuildChannel>, String> {
    // Read remote config
    let output = crate::cli_runner::run_openclaw_remote(
        &pool,
        &host_id,
        &["config", "get", "channels.discord", "--json"],
    )
    .await?;
    let bindings_output = crate::cli_runner::run_openclaw_remote(
        &pool,
        &host_id,
        &["config", "get", "bindings", "--json"],
    )
    .await?;
    let cli_discord = if output.exit_code == 0 {
        crate::cli_runner::parse_json_output(&output).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let cli_has_discord =
        cli_discord.get("guilds").is_some() || cli_discord.get("accounts").is_some();
    let config_fallback =
        if cli_has_discord && output.exit_code == 0 && bindings_output.exit_code == 0 {
            None
        } else {
            remote_read_openclaw_config_text_and_json(&pool, &host_id)
                .await
                .ok()
                .map(|(_, _, cfg)| cfg)
        };
    let (fallback_discord_section, fallback_bindings_section) = config_fallback
        .as_ref()
        .map(discord_sections_from_openclaw_config)
        .unwrap_or_else(|| (Value::Null, Value::Array(Vec::new())));
    let discord_section = if cli_has_discord {
        cli_discord
    } else {
        fallback_discord_section
    };
    let bindings_section = if bindings_output.exit_code == 0 {
        crate::cli_runner::parse_json_output(&bindings_output).unwrap_or(fallback_bindings_section)
    } else {
        fallback_bindings_section
    };
    let cfg = serde_json::json!({
        "channels": { "discord": discord_section },
        "bindings": bindings_section
    });

    let core_channels =
        clawpal_core::discovery::parse_guild_channels(&cfg.to_string()).unwrap_or_default();

    // Read remote cache for resolved names
    let cached: Vec<DiscordGuildChannel> = pool
        .sftp_read(&host_id, "~/.clawpal/discord-guild-channels.json")
        .await
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();

    // Merge: prefer cached names, fill in config-derived entries
    let mut cache_map: std::collections::HashMap<(String, String), DiscordGuildChannel> = cached
        .into_iter()
        .map(|e| ((e.guild_id.clone(), e.channel_id.clone()), e))
        .collect();

    // Enrich guild names from config (slug/name fields)
    let discord_cfg = cfg.get("channels").and_then(|c| c.get("discord"));
    let guild_name_fallback = collect_discord_config_guild_name_fallbacks(discord_cfg);

    let mut result: Vec<DiscordGuildChannel> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for ch in &core_channels {
        let key = (ch.guild_id.clone(), ch.channel_id.clone());
        if !seen.insert(key.clone()) {
            continue;
        }
        if let Some(cached_entry) = cache_map.remove(&key) {
            result.push(cached_entry);
        } else {
            let guild_name = guild_name_fallback
                .get(&ch.guild_id)
                .cloned()
                .unwrap_or_else(|| ch.guild_name.clone());
            result.push(DiscordGuildChannel {
                guild_id: ch.guild_id.clone(),
                guild_name,
                channel_id: ch.channel_id.clone(),
                channel_name: ch.channel_name.clone(),
                default_agent_id: None,
                resolution_warning: None,
            });
        }
    }

    for (key, entry) in cache_map {
        if seen.insert(key) {
            result.push(entry);
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn refresh_discord_guild_channels(
    force_refresh: bool,
) -> Result<Vec<DiscordGuildChannel>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = resolve_paths();
        ensure_dirs(&paths)?;
        let cfg = read_openclaw_config(&paths)?;

        let discord_cfg = cfg.get("channels").and_then(|c| c.get("discord"));
        let configured_single_guild_id = discord_cfg
            .and_then(|d| d.get("guilds"))
            .and_then(Value::as_object)
            .and_then(|guilds| {
                if guilds.len() == 1 {
                    guilds.keys().next().cloned()
                } else {
                    None
                }
            });

        // Extract bot token — used by Fallback A (fetch channels via Discord REST when
        // config has no explicit channel list).
        // Guild *name* resolution is handled by the frontend (discord-id-cache.ts).
        let bot_token = extract_discord_bot_token(discord_cfg);

        let cache_file = paths.clawpal_dir.join("discord-guild-channels.json");

        // TTL gate: return cached data if it is fresh and caller did not force a refresh.
        if !force_refresh && cache_file.exists() {
            if let Ok(meta) = fs::metadata(&cache_file) {
                if let Ok(elapsed) = meta.modified().and_then(|m| {
                    m.elapsed()
                        .map_err(|e| std::io::Error::other(e.to_string()))
                }) {
                    if elapsed.as_secs() < DISCORD_CACHE_TTL_SECS {
                        let text = fs::read_to_string(&cache_file).unwrap_or_default();
                        let entries: Vec<DiscordGuildChannel> =
                            serde_json::from_str(&text).unwrap_or_default();
                        if !entries.is_empty() {
                            return Ok(entries);
                        }
                    }
                }
            }
        }

        let mut guild_name_fallback_map = fs::read_to_string(&cache_file)
            .ok()
            .map(|text| parse_discord_cache_guild_name_fallbacks(&text))
            .unwrap_or_default();
        guild_name_fallback_map.extend(collect_discord_config_guild_name_fallbacks(discord_cfg));

        let mut entries: Vec<DiscordGuildChannel> = Vec::new();
        let mut channel_ids: Vec<String> = Vec::new();

        // Helper: collect guilds from a guilds object
        let mut collect_guilds = |guilds: &serde_json::Map<String, Value>| {
            for (guild_id, guild_val) in guilds {
                let guild_name = guild_val
                    .get("slug")
                    .or_else(|| guild_val.get("name"))
                    .and_then(Value::as_str)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| guild_id.clone());

                if let Some(channels) = guild_val.get("channels").and_then(Value::as_object) {
                    for (channel_id, _channel_val) in channels {
                        // Skip glob/wildcard patterns (e.g. "*") — not real channel IDs
                        if channel_id.contains('*') || channel_id.contains('?') {
                            continue;
                        }
                        if entries
                            .iter()
                            .any(|e| e.guild_id == *guild_id && e.channel_id == *channel_id)
                        {
                            continue;
                        }
                        channel_ids.push(channel_id.clone());
                        entries.push(DiscordGuildChannel {
                            guild_id: guild_id.clone(),
                            guild_name: guild_name.clone(),
                            channel_id: channel_id.clone(),
                            channel_name: channel_id.clone(),
                            default_agent_id: None,
                            resolution_warning: None,
                        });
                    }
                }
            }
        };

        // Collect from channels.discord.guilds (top-level structured config)
        if let Some(guilds) = discord_cfg
            .and_then(|d| d.get("guilds"))
            .and_then(Value::as_object)
        {
            collect_guilds(guilds);
        }

        // Collect from channels.discord.accounts.<accountId>.guilds (multi-account config)
        if let Some(accounts) = discord_cfg
            .and_then(|d| d.get("accounts"))
            .and_then(Value::as_object)
        {
            for (_account_id, account_val) in accounts {
                if let Some(guilds) = account_val.get("guilds").and_then(Value::as_object) {
                    collect_guilds(guilds);
                }
            }
        }

        drop(collect_guilds); // Release mutable borrows before bindings section

        // Also collect from bindings array (users may only have bindings, no guilds map)
        if let Some(bindings) = cfg.get("bindings").and_then(Value::as_array) {
            for b in bindings {
                let m = match b.get("match") {
                    Some(m) => m,
                    None => continue,
                };
                if m.get("channel").and_then(Value::as_str) != Some("discord") {
                    continue;
                }
                let guild_id = match m.get("guildId") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Number(n)) => n.to_string(),
                    _ => continue,
                };
                let channel_id = match m.pointer("/peer/id") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Number(n)) => n.to_string(),
                    _ => continue,
                };
                // Skip if already collected from guilds map
                if entries
                    .iter()
                    .any(|e| e.guild_id == guild_id && e.channel_id == channel_id)
                {
                    continue;
                }
                channel_ids.push(channel_id.clone());
                entries.push(DiscordGuildChannel {
                    guild_id: guild_id.clone(),
                    guild_name: guild_id.clone(),
                    channel_id: channel_id.clone(),
                    channel_name: channel_id.clone(),
                    default_agent_id: None,
                    resolution_warning: None,
                });
            }
        }

        // Fallback A: fetch channels from Discord REST for guilds that have no entries yet.
        // Build a guild_id -> token mapping so each guild uses the correct bot token.
        {
            let mut guild_token_map: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();

            // Map guilds from accounts to their respective tokens
            if let Some(accounts) = discord_cfg
                .and_then(|d| d.get("accounts"))
                .and_then(Value::as_object)
            {
                for (_acct_id, acct_val) in accounts {
                    let acct_token = acct_val
                        .get("token")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string());
                    if let Some(token) = acct_token {
                        if let Some(guilds) = acct_val.get("guilds").and_then(Value::as_object) {
                            for guild_id in guilds.keys() {
                                guild_token_map
                                    .entry(guild_id.clone())
                                    .or_insert_with(|| token.clone());
                            }
                        }
                    }
                }
            }

            // Also map top-level guilds to the top-level bot token
            if let Some(token) = &bot_token {
                let configured_guild_ids = collect_discord_config_guild_ids(discord_cfg);
                for guild_id in &configured_guild_ids {
                    guild_token_map
                        .entry(guild_id.clone())
                        .or_insert_with(|| token.clone());
                }
            }

            for (guild_id, token) in &guild_token_map {
                // Skip guilds that already have entries from config/bindings
                if entries.iter().any(|e| e.guild_id == *guild_id) {
                    continue;
                }
                if let Ok(channels) = fetch_discord_guild_channels(token, guild_id) {
                    for (channel_id, channel_name) in channels {
                        if entries
                            .iter()
                            .any(|e| e.guild_id == *guild_id && e.channel_id == channel_id)
                        {
                            continue;
                        }
                        channel_ids.push(channel_id.clone());
                        entries.push(DiscordGuildChannel {
                            guild_id: guild_id.clone(),
                            guild_name: guild_id.clone(),
                            channel_id,
                            channel_name,
                            default_agent_id: None,
                            resolution_warning: None,
                        });
                    }
                }
            }
        }

        // Fallback B: query channel ids from directory and keep compatibility
        // with existing cache shape when config has no explicit channel map.
        if channel_ids.is_empty() {
            if let Ok(output) = run_openclaw_raw(&[
                "directory",
                "groups",
                "list",
                "--channel",
                "discord",
                "--json",
            ]) {
                for channel_id in parse_directory_group_channel_ids(&output.stdout) {
                    if entries.iter().any(|e| e.channel_id == channel_id) {
                        continue;
                    }
                    let (guild_id, guild_name) =
                        if let Some(gid) = configured_single_guild_id.clone() {
                            (gid.clone(), gid)
                        } else {
                            ("discord".to_string(), "Discord".to_string())
                        };
                    channel_ids.push(channel_id.clone());
                    entries.push(DiscordGuildChannel {
                        guild_id,
                        guild_name,
                        channel_id: channel_id.clone(),
                        channel_name: channel_id,
                        default_agent_id: None,
                        resolution_warning: None,
                    });
                }
            }
        }

        if entries.is_empty() {
            return Ok(Vec::new());
        }

        // Load id→name cache to avoid repeated network requests for known IDs.
        let id_cache_path = paths.clawpal_dir.join("discord-id-cache.json");
        let mut id_cache =
            DiscordIdCache::from_str(&fs::read_to_string(&id_cache_path).unwrap_or_default());
        let now_secs = unix_now_secs();

        // Resolve channel names: apply id cache first, then call CLI for misses.
        {
            for entry in &mut entries {
                if entry.channel_name == entry.channel_id {
                    if let Some(name) =
                        id_cache.get_channel_name(&entry.channel_id, now_secs, force_refresh)
                    {
                        entry.channel_name = name.to_string();
                    }
                }
            }
            let uncached_ids: Vec<String> = channel_ids
                .iter()
                .filter(|id| {
                    id_cache
                        .get_channel_name(id, now_secs, force_refresh)
                        .is_none()
                })
                .cloned()
                .collect();
            if !uncached_ids.is_empty() {
                let mut args = vec![
                    "channels",
                    "resolve",
                    "--json",
                    "--channel",
                    "discord",
                    "--kind",
                    "auto",
                ];
                let id_refs: Vec<&str> = uncached_ids.iter().map(String::as_str).collect();
                args.extend_from_slice(&id_refs);
                if let Ok(output) = run_openclaw_raw(&args) {
                    if let Some(name_map) = parse_resolve_name_map(&output.stdout) {
                        for entry in &mut entries {
                            if let Some(name) = name_map.get(&entry.channel_id) {
                                entry.channel_name = name.clone();
                                id_cache.put_channel(
                                    entry.channel_id.clone(),
                                    name.clone(),
                                    now_secs,
                                );
                            }
                        }
                    }
                }
            }
        }

        // Resolve guild names via Discord REST API, using id cache to skip known guilds.
        {
            let unresolved: Vec<String> = entries
                .iter()
                .filter(|e| e.guild_name == e.guild_id)
                .map(|e| e.guild_id.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            // Apply already-cached names.
            for entry in &mut entries {
                if entry.guild_name == entry.guild_id {
                    if let Some(name) =
                        id_cache.get_guild_name(&entry.guild_id, now_secs, force_refresh)
                    {
                        entry.guild_name = name.to_string();
                    }
                }
            }

            // Fetch from Discord REST for guilds still unresolved after cache check.
            let needs_rest: Vec<String> = unresolved
                .into_iter()
                .filter(|gid| {
                    id_cache
                        .get_guild_name(gid, now_secs, force_refresh)
                        .is_none()
                })
                .collect();
            if let Some(token) = &bot_token {
                if !needs_rest.is_empty() {
                    let mut guild_name_map = std::collections::HashMap::new();
                    for gid in &needs_rest {
                        if let Ok(name) = fetch_discord_guild_name(token, gid) {
                            guild_name_map.insert(gid.clone(), name);
                        }
                    }
                    for (gid, name) in &guild_name_map {
                        id_cache.put_guild(gid.clone(), name.clone(), now_secs);
                    }
                    for entry in &mut entries {
                        if let Some(name) = guild_name_map.get(&entry.guild_id) {
                            entry.guild_name = name.clone();
                        }
                    }
                }
            }
        }

        // Config-derived slug/name fallbacks (last resort for guilds still showing as IDs).
        for entry in &mut entries {
            if entry.guild_name == entry.guild_id {
                if let Some(name) = guild_name_fallback_map.get(&entry.guild_id) {
                    entry.guild_name = name.clone();
                }
            }
        }

        // Resolve default agent per guild from account config + bindings
        {
            // Build account_id -> default agent_id from bindings (account-level, no peer)
            let mut account_agent_map: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            if let Some(bindings) = cfg.get("bindings").and_then(Value::as_array) {
                for b in bindings {
                    let m = match b.get("match") {
                        Some(m) => m,
                        None => continue,
                    };
                    if m.get("channel").and_then(Value::as_str) != Some("discord") {
                        continue;
                    }
                    let account_id = match m.get("accountId").and_then(Value::as_str) {
                        Some(s) => s,
                        None => continue,
                    };
                    if m.get("peer").and_then(|p| p.get("id")).is_some() {
                        continue;
                    }
                    if let Some(agent_id) = b.get("agentId").and_then(Value::as_str) {
                        account_agent_map
                            .entry(account_id.to_string())
                            .or_insert_with(|| agent_id.to_string());
                    }
                }
            }
            let mut guild_default_agent: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            if let Some(accounts) = discord_cfg
                .and_then(|d| d.get("accounts"))
                .and_then(Value::as_object)
            {
                for (account_id, account_val) in accounts {
                    let agent = account_agent_map
                        .get(account_id)
                        .cloned()
                        .unwrap_or_else(|| account_id.clone());
                    if let Some(guilds) = account_val.get("guilds").and_then(Value::as_object) {
                        for guild_id in guilds.keys() {
                            guild_default_agent
                                .entry(guild_id.clone())
                                .or_insert(agent.clone());
                        }
                    }
                }
            }
            for entry in &mut entries {
                if entry.default_agent_id.is_none() {
                    if let Some(agent_id) = guild_default_agent.get(&entry.guild_id) {
                        entry.default_agent_id = Some(agent_id.clone());
                    }
                }
            }
        }

        // Persist to cache
        let json = serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())?;
        write_text(&cache_file, &json)?;
        let _ = write_text(&id_cache_path, &id_cache.to_json());

        Ok(entries)
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn list_bindings_with_cache(
    cache: &crate::cli_runner::CliCache,
) -> Result<Vec<Value>, String> {
    let cache_key = local_cli_cache_key("bindings");
    if let Some(cached) = cache.get(&cache_key, None) {
        return serde_json::from_str(&cached).map_err(|e| e.to_string());
    }
    let cache = cache.clone();
    let cache_key_cloned = cache_key.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let output = crate::cli_runner::run_openclaw(&["config", "get", "bindings", "--json"])?;
        // "bindings" may not exist yet — treat "not found" as empty
        if output.exit_code != 0 {
            let msg = format!("{} {}", output.stderr, output.stdout).to_lowercase();
            if msg.contains("not found") {
                return Ok(Vec::new());
            }
        }
        let json = crate::cli_runner::parse_json_output(&output)?;
        let result = json.as_array().cloned().unwrap_or_default();
        if let Ok(serialized) = serde_json::to_string(&result) {
            cache.set(cache_key_cloned, serialized);
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_bindings(
    cache: tauri::State<'_, crate::cli_runner::CliCache>,
) -> Result<Vec<Value>, String> {
    list_bindings_with_cache(cache.inner()).await
}

pub async fn list_agents_overview_with_cache(
    cache: &crate::cli_runner::CliCache,
) -> Result<Vec<AgentOverview>, String> {
    let cache_key = local_cli_cache_key("agents-list");
    if let Some(cached) = cache.get(&cache_key, None) {
        return serde_json::from_str(&cached).map_err(|e| e.to_string());
    }
    let cache = cache.clone();
    let cache_key_cloned = cache_key.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let output = crate::cli_runner::run_openclaw(&["agents", "list", "--json"])?;
        let json = crate::cli_runner::parse_json_output(&output)?;
        let result = parse_agents_cli_output(&json, None)?;
        if let Ok(serialized) = serde_json::to_string(&result) {
            cache.set(cache_key_cloned, serialized);
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_agents_overview(
    cache: tauri::State<'_, crate::cli_runner::CliCache>,
) -> Result<Vec<AgentOverview>, String> {
    list_agents_overview_with_cache(cache.inner()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashSet;

    // ── extract_discord_bot_token ─────────────────────────────────────────────

    #[test]
    fn extract_bot_token_from_top_level_bot_token_field() {
        let cfg = json!({ "botToken": "token-abc" });
        assert_eq!(
            extract_discord_bot_token(Some(&cfg)).as_deref(),
            Some("token-abc")
        );
    }

    #[test]
    fn extract_bot_token_from_top_level_token_field() {
        let cfg = json!({ "token": "token-xyz" });
        assert_eq!(
            extract_discord_bot_token(Some(&cfg)).as_deref(),
            Some("token-xyz")
        );
    }

    #[test]
    fn extract_bot_token_falls_back_to_account_token() {
        let cfg = json!({
            "accounts": {
                "acct1": { "token": "acct-token" }
            }
        });
        assert_eq!(
            extract_discord_bot_token(Some(&cfg)).as_deref(),
            Some("acct-token")
        );
    }

    #[test]
    fn extract_bot_token_skips_empty_account_token() {
        let cfg = json!({
            "accounts": {
                "acct1": { "token": "" },
                "acct2": { "token": "real-token" }
            }
        });
        assert_eq!(
            extract_discord_bot_token(Some(&cfg)).as_deref(),
            Some("real-token")
        );
    }

    #[test]
    fn extract_bot_token_returns_none_when_absent() {
        let cfg = json!({ "guilds": {} });
        assert_eq!(extract_discord_bot_token(Some(&cfg)), None);
        assert_eq!(extract_discord_bot_token(None), None);
    }

    // ── existing tests ────────────────────────────────────────────────────────

    #[test]
    fn discord_sections_from_openclaw_config_extracts_discord_and_bindings() {
        let cfg = json!({
            "channels": {
                "discord": {
                    "guilds": {
                        "guild-recipe-lab": {
                            "name": "Recipe Lab",
                            "channels": {
                                "channel-general": { "systemPrompt": "" }
                            }
                        }
                    }
                }
            },
            "bindings": [
                { "agentId": "main" }
            ]
        });

        let (discord, bindings) = discord_sections_from_openclaw_config(&cfg);

        assert_eq!(
            discord
                .pointer("/guilds/guild-recipe-lab/name")
                .and_then(Value::as_str),
            Some("Recipe Lab")
        );
        assert_eq!(bindings.as_array().map(|items| items.len()), Some(1));
    }

    #[test]
    fn agent_overviews_from_openclaw_config_marks_online_agents() {
        let cfg = json!({
            "agents": {
                "list": [
                    { "id": "main", "model": "anthropic/claude-sonnet-4-20250514" },
                    { "id": "helper", "identityName": "Helper", "model": "openai/gpt-4o" }
                ]
            }
        });
        let online_set = HashSet::from([String::from("helper")]);

        let agents = agent_overviews_from_openclaw_config(&cfg, &online_set);

        assert_eq!(agents.len(), 2);
        assert!(
            !agents
                .iter()
                .find(|agent| agent.id == "main")
                .unwrap()
                .online
        );
        let helper = agents.iter().find(|agent| agent.id == "helper").unwrap();
        assert!(helper.online);
        assert_eq!(helper.name.as_deref(), Some("Helper"));
    }

    #[test]
    fn summarize_resolution_error_both_empty() {
        assert_eq!(super::summarize_resolution_error("", ""), "unknown error");
    }

    #[test]
    fn summarize_resolution_error_stderr_only() {
        let result = super::summarize_resolution_error("connection refused", "");
        assert!(result.contains("connection refused"));
    }

    #[test]
    fn summarize_resolution_error_combined() {
        let result = super::summarize_resolution_error("err", "out");
        assert!(result.contains("err"));
        assert!(result.contains("out"));
    }

    #[test]
    fn append_resolution_warning_to_none() {
        let mut target: Option<String> = None;
        super::append_resolution_warning(&mut target, "warning msg");
        assert_eq!(target.as_deref(), Some("warning msg"));
    }

    #[test]
    fn append_resolution_warning_duplicate_skipped() {
        let mut target = Some("existing warning".into());
        super::append_resolution_warning(&mut target, "existing warning");
        assert_eq!(target.as_deref(), Some("existing warning"));
    }

    #[test]
    fn append_resolution_warning_new_appended() {
        let mut target = Some("first".into());
        super::append_resolution_warning(&mut target, "second");
        let value = target.unwrap();
        assert!(value.contains("first"));
        assert!(value.contains("second"));
    }

    #[test]
    fn append_resolution_warning_empty_ignored() {
        let mut target: Option<String> = None;
        super::append_resolution_warning(&mut target, "");
        assert!(target.is_none());
        super::append_resolution_warning(&mut target, "   ");
        assert!(target.is_none());
    }
}
