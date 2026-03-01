use super::*;

#[tauri::command]
pub async fn remote_manage_rescue_bot(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    action: String,
    profile: Option<String>,
    rescue_port: Option<u16>,
) -> Result<RescueBotManageResult, String> {
    let action = RescueBotAction::parse(&action)?;
    let profile = profile
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or("rescue")
        .to_string();

    let main_port = match remote_resolve_openclaw_config_path(&pool, &host_id).await {
        Ok(path) => match pool.sftp_read(&host_id, &path).await {
            Ok(raw) => {
                let cfg = clawpal_core::config::parse_config_json5(&raw);
                clawpal_core::config::resolve_gateway_port(&cfg)
            }
            Err(_) => 18789,
        },
        Err(_) => 18789,
    };
    let (already_configured, existing_port) =
        resolve_remote_rescue_profile_state(&pool, &host_id, &profile).await?;
    let should_configure = !already_configured || action == RescueBotAction::Set;
    let rescue_port = if should_configure {
        rescue_port.unwrap_or_else(|| clawpal_core::doctor::suggest_rescue_port(main_port))
    } else {
        existing_port
            .or(rescue_port)
            .unwrap_or_else(|| clawpal_core::doctor::suggest_rescue_port(main_port))
    };
    let min_recommended_port = main_port.saturating_add(20);

    if should_configure && matches!(action, RescueBotAction::Set | RescueBotAction::Activate) {
        clawpal_core::doctor::ensure_rescue_port_spacing(main_port, rescue_port)?;
    }

    if action == RescueBotAction::Status && !already_configured {
        return Ok(RescueBotManageResult {
            action: action.as_str().into(),
            profile,
            main_port,
            rescue_port,
            min_recommended_port,
            was_already_configured: false,
            commands: Vec::new(),
        });
    }

    let plan = build_rescue_bot_command_plan(action, &profile, rescue_port, should_configure);
    let mut commands = Vec::new();
    for command in plan {
        let result = run_remote_rescue_bot_command(&pool, &host_id, command).await?;
        if result.output.exit_code != 0 {
            if action == RescueBotAction::Status {
                commands.push(result);
                break;
            }
            if is_rescue_cleanup_noop(action, &result.command, &result.output) {
                commands.push(result);
                continue;
            }
            if action == RescueBotAction::Activate
                && is_gateway_restart_command(&result.command)
                && is_gateway_restart_timeout(&result.output)
            {
                commands.push(result);
                run_remote_gateway_restart_fallback(&pool, &host_id, &profile, &mut commands)
                    .await?;
                continue;
            }
            return Err(command_failure_message(&result.command, &result.output));
        }
        commands.push(result);
    }

    Ok(RescueBotManageResult {
        action: action.as_str().into(),
        profile,
        main_port,
        rescue_port,
        min_recommended_port,
        was_already_configured: already_configured,
        commands,
    })
}

#[tauri::command]
pub async fn remote_diagnose_primary_via_rescue(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    target_profile: Option<String>,
    rescue_profile: Option<String>,
) -> Result<RescuePrimaryDiagnosisResult, String> {
    let target_profile = normalize_profile_name(target_profile.as_deref(), "primary");
    let rescue_profile = normalize_profile_name(rescue_profile.as_deref(), "rescue");
    diagnose_primary_via_rescue_remote(&pool, &host_id, &target_profile, &rescue_profile).await
}

#[tauri::command]
pub async fn remote_repair_primary_via_rescue(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    target_profile: Option<String>,
    rescue_profile: Option<String>,
    issue_ids: Option<Vec<String>>,
) -> Result<RescuePrimaryRepairResult, String> {
    let target_profile = normalize_profile_name(target_profile.as_deref(), "primary");
    let rescue_profile = normalize_profile_name(rescue_profile.as_deref(), "rescue");
    repair_primary_via_rescue_remote(
        &pool,
        &host_id,
        &target_profile,
        &rescue_profile,
        issue_ids.unwrap_or_default(),
    )
    .await
}
