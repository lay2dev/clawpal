use super::*;

fn escape_single_quoted_shell(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

async fn remote_log_helper_event(pool: &SshConnectionPool, host_id: &str, message: &str) {
    let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let line = format!("[{ts}] {message}");
    let escaped = escape_single_quoted_shell(&line);
    let cmd = format!(
        "mkdir -p \"$HOME/.clawpal/logs\"; printf '%s\\n' '{}' >> \"$HOME/.clawpal/logs/helper.log\"",
        escaped
    );
    let _ = pool.exec(host_id, &cmd).await;
}

#[tauri::command]
pub async fn remote_manage_rescue_bot(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    action: String,
    profile: Option<String>,
    rescue_port: Option<u16>,
) -> Result<RescueBotManageResult, String> {
    timed_async!("remote_manage_rescue_bot", {
        let action_label = action.clone();
        let profile_label = profile.clone().unwrap_or_else(|| "rescue".into());
        remote_log_helper_event(
            &pool,
            &host_id,
            &format!(
                "[remote:{host_id}] manage_rescue_bot start action={} profile={}",
                action_label, profile_label
            ),
        )
        .await;

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
        let should_configure = !already_configured
            || action == RescueBotAction::Set
            || action == RescueBotAction::Activate;
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
            let runtime_state = infer_rescue_bot_runtime_state(false, None, None);
            return Ok(RescueBotManageResult {
                action: action.as_str().into(),
                profile,
                main_port,
                rescue_port,
                min_recommended_port,
                configured: false,
                active: false,
                runtime_state,
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

        let configured = match action {
            RescueBotAction::Unset => false,
            RescueBotAction::Activate | RescueBotAction::Set | RescueBotAction::Deactivate => true,
            RescueBotAction::Status => already_configured,
        };
        let mut status_output = commands
            .iter()
            .rev()
            .find(|result| {
                result
                    .command
                    .windows(2)
                    .any(|window| window[0] == "gateway" && window[1] == "status")
            })
            .map(|result| &result.output);
        if action == RescueBotAction::Activate {
            let active_now = status_output
                .map(|output| infer_rescue_bot_runtime_state(true, Some(output), None) == "active")
                .unwrap_or(false);
            if !active_now {
                let probe_status = build_gateway_status_command(&profile, true);
                if let Ok(result) =
                    run_remote_rescue_bot_command(&pool, &host_id, probe_status).await
                {
                    commands.push(result);
                    status_output = commands
                        .iter()
                        .rev()
                        .find(|result| {
                            result
                                .command
                                .windows(2)
                                .any(|window| window[0] == "gateway" && window[1] == "status")
                        })
                        .map(|result| &result.output);
                }
            }
        }
        let runtime_state = infer_rescue_bot_runtime_state(configured, status_output, None);
        let active = runtime_state == "active";

        let result = RescueBotManageResult {
            action: action.as_str().into(),
            profile,
            main_port,
            rescue_port,
            min_recommended_port,
            configured,
            active,
            runtime_state,
            was_already_configured: already_configured,
            commands,
        };

        remote_log_helper_event(
        &pool,
        &host_id,
        &format!(
            "[remote:{host_id}] manage_rescue_bot success action={} profile={} state={} configured={} active={}",
            action_label, result.profile, result.runtime_state, result.configured, result.active
        ),
    )
    .await;

        Ok(result)
    })
}

#[tauri::command]
pub async fn remote_get_rescue_bot_status(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    profile: Option<String>,
    rescue_port: Option<u16>,
) -> Result<RescueBotManageResult, String> {
    timed_async!("remote_get_rescue_bot_status", {
        remote_manage_rescue_bot(pool, host_id, "status".to_string(), profile, rescue_port).await
    })
}

#[tauri::command]
pub async fn remote_diagnose_primary_via_rescue(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    target_profile: Option<String>,
    rescue_profile: Option<String>,
) -> Result<RescuePrimaryDiagnosisResult, String> {
    timed_async!("remote_diagnose_primary_via_rescue", {
        let target_profile = normalize_profile_name(target_profile.as_deref(), "primary");
        let rescue_profile = normalize_profile_name(rescue_profile.as_deref(), "rescue");
        remote_log_helper_event(
            &pool,
            &host_id,
            &format!(
                "[remote:{host_id}] diagnose_primary_via_rescue start target={} rescue={}",
                target_profile, rescue_profile
            ),
        )
        .await;
        let result =
            diagnose_primary_via_rescue_remote(&pool, &host_id, &target_profile, &rescue_profile)
                .await;
        match &result {
            Ok(summary) => {
                remote_log_helper_event(
                    &pool,
                    &host_id,
                    &format!(
                        "[remote:{host_id}] diagnose_primary_via_rescue success target={} rescue={} status={} issues={}",
                        summary.target_profile,
                        summary.rescue_profile,
                        summary.summary.status,
                        summary.issues.len()
                    ),
                )
                .await;
            }
            Err(error) => {
                remote_log_helper_event(
                    &pool,
                    &host_id,
                    &format!(
                        "[remote:{host_id}] diagnose_primary_via_rescue failed target={} rescue={} error={}",
                        target_profile, rescue_profile, error
                    ),
                )
                .await;
            }
        }
        result
    })
}

#[tauri::command]
pub async fn remote_repair_primary_via_rescue(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    target_profile: Option<String>,
    rescue_profile: Option<String>,
    issue_ids: Option<Vec<String>>,
) -> Result<RescuePrimaryRepairResult, String> {
    timed_async!("remote_repair_primary_via_rescue", {
        let target_profile = normalize_profile_name(target_profile.as_deref(), "primary");
        let rescue_profile = normalize_profile_name(rescue_profile.as_deref(), "rescue");
        let requested_issue_count = issue_ids.as_ref().map_or(0, Vec::len);
        remote_log_helper_event(
            &pool,
            &host_id,
            &format!(
                "[remote:{host_id}] repair_primary_via_rescue start target={} rescue={} requested_issues={}",
                target_profile, rescue_profile, requested_issue_count
            ),
        )
        .await;
        let result = repair_primary_via_rescue_remote(
            &pool,
            &host_id,
            &target_profile,
            &rescue_profile,
            issue_ids.unwrap_or_default(),
        )
        .await;
        match &result {
            Ok(summary) => {
                remote_log_helper_event(
                    &pool,
                    &host_id,
                    &format!(
                        "[remote:{host_id}] repair_primary_via_rescue success target={} rescue={} applied={} failed={} skipped={}",
                        summary.target_profile,
                        summary.rescue_profile,
                        summary.applied_issue_ids.len(),
                        summary.failed_issue_ids.len(),
                        summary.skipped_issue_ids.len()
                    ),
                )
                .await;
            }
            Err(error) => {
                remote_log_helper_event(
                    &pool,
                    &host_id,
                    &format!(
                        "[remote:{host_id}] repair_primary_via_rescue failed target={} rescue={} error={}",
                        target_profile, rescue_profile, error
                    ),
                )
                .await;
            }
        }
        result
    })
}

#[tauri::command]
pub async fn manage_rescue_bot(
    action: String,
    profile: Option<String>,
    rescue_port: Option<u16>,
) -> Result<RescueBotManageResult, String> {
    timed_async!("manage_rescue_bot", {
        let action_label = action.clone();
        let profile_label = profile.clone().unwrap_or_else(|| "rescue".into());
        crate::logging::log_helper(&format!(
            "[local] manage_rescue_bot start action={} profile={}",
            action_label, profile_label
        ));
        let result = tauri::async_runtime::spawn_blocking(move || {
            let action = RescueBotAction::parse(&action)?;
            let profile = profile
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .unwrap_or("rescue")
                .to_string();

            let main_port = read_openclaw_config(&resolve_paths())
                .map(|cfg| clawpal_core::doctor::resolve_gateway_port_from_config(&cfg))
                .unwrap_or(18789);
            let (already_configured, existing_port) = resolve_local_rescue_profile_state(&profile)?;
            let should_configure = !already_configured
                || action == RescueBotAction::Set
                || action == RescueBotAction::Activate;
            let rescue_port = if should_configure {
                rescue_port.unwrap_or_else(|| clawpal_core::doctor::suggest_rescue_port(main_port))
            } else {
                existing_port
                    .or(rescue_port)
                    .unwrap_or_else(|| clawpal_core::doctor::suggest_rescue_port(main_port))
            };
            let min_recommended_port = main_port.saturating_add(20);

            if should_configure
                && matches!(action, RescueBotAction::Set | RescueBotAction::Activate)
            {
                clawpal_core::doctor::ensure_rescue_port_spacing(main_port, rescue_port)?;
            }

            if action == RescueBotAction::Status && !already_configured {
                let runtime_state = infer_rescue_bot_runtime_state(false, None, None);
                return Ok(RescueBotManageResult {
                    action: action.as_str().into(),
                    profile,
                    main_port,
                    rescue_port,
                    min_recommended_port,
                    configured: false,
                    active: false,
                    runtime_state,
                    was_already_configured: false,
                    commands: Vec::new(),
                });
            }

            let plan =
                build_rescue_bot_command_plan(action, &profile, rescue_port, should_configure);
            let mut commands = Vec::new();

            for command in plan {
                let result = run_local_rescue_bot_command(command)?;
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
                        run_local_gateway_restart_fallback(&profile, &mut commands)?;
                        continue;
                    }
                    return Err(command_failure_message(&result.command, &result.output));
                }
                commands.push(result);
            }

            let configured = match action {
                RescueBotAction::Unset => false,
                RescueBotAction::Activate | RescueBotAction::Set | RescueBotAction::Deactivate => {
                    true
                }
                RescueBotAction::Status => already_configured,
            };
            let mut status_output = commands
                .iter()
                .rev()
                .find(|result| {
                    result
                        .command
                        .windows(2)
                        .any(|window| window[0] == "gateway" && window[1] == "status")
                })
                .map(|result| &result.output);
            if action == RescueBotAction::Activate {
                let active_now = status_output
                    .map(|output| {
                        infer_rescue_bot_runtime_state(true, Some(output), None) == "active"
                    })
                    .unwrap_or(false);
                if !active_now {
                    let probe_status = build_gateway_status_command(&profile, true);
                    if let Ok(result) = run_local_rescue_bot_command(probe_status) {
                        commands.push(result);
                        status_output = commands
                            .iter()
                            .rev()
                            .find(|result| {
                                result
                                    .command
                                    .windows(2)
                                    .any(|window| window[0] == "gateway" && window[1] == "status")
                            })
                            .map(|result| &result.output);
                    }
                }
            }
            let runtime_state = infer_rescue_bot_runtime_state(configured, status_output, None);
            let active = runtime_state == "active";

            Ok(RescueBotManageResult {
                action: action.as_str().into(),
                profile,
                main_port,
                rescue_port,
                min_recommended_port,
                configured,
                active,
                runtime_state,
                was_already_configured: already_configured,
                commands,
            })
        })
        .await
        .map_err(|e| e.to_string())?;

        match &result {
        Ok(summary) => crate::logging::log_helper(&format!(
            "[local] manage_rescue_bot success action={} profile={} state={} configured={} active={}",
            action_label, summary.profile, summary.runtime_state, summary.configured, summary.active
        )),
        Err(error) => crate::logging::log_helper(&format!(
            "[local] manage_rescue_bot failed action={} profile={} error={}",
            action_label, profile_label, error
        )),
    }

        result
    })
}

#[tauri::command]
pub async fn get_rescue_bot_status(
    profile: Option<String>,
    rescue_port: Option<u16>,
) -> Result<RescueBotManageResult, String> {
    timed_async!("get_rescue_bot_status", {
        manage_rescue_bot("status".to_string(), profile, rescue_port).await
    })
}

#[tauri::command]
pub async fn diagnose_primary_via_rescue(
    target_profile: Option<String>,
    rescue_profile: Option<String>,
) -> Result<RescuePrimaryDiagnosisResult, String> {
    timed_async!("diagnose_primary_via_rescue", {
        let target_label = normalize_profile_name(target_profile.as_deref(), "primary");
        let rescue_label = normalize_profile_name(rescue_profile.as_deref(), "rescue");
        crate::logging::log_helper(&format!(
            "[local] diagnose_primary_via_rescue start target={} rescue={}",
            target_label, rescue_label
        ));
        let result = tauri::async_runtime::spawn_blocking(move || {
            let target_profile = normalize_profile_name(target_profile.as_deref(), "primary");
            let rescue_profile = normalize_profile_name(rescue_profile.as_deref(), "rescue");
            diagnose_primary_via_rescue_local(&target_profile, &rescue_profile)
        })
        .await
        .map_err(|e| e.to_string())?;

        match &result {
            Ok(summary) => crate::logging::log_helper(&format!(
            "[local] diagnose_primary_via_rescue success target={} rescue={} status={} issues={}",
            summary.target_profile,
            summary.rescue_profile,
            summary.summary.status,
            summary.issues.len()
        )),
            Err(error) => crate::logging::log_helper(&format!(
                "[local] diagnose_primary_via_rescue failed target={} rescue={} error={}",
                target_label, rescue_label, error
            )),
        }

        result
    })
}

#[tauri::command]
pub async fn repair_primary_via_rescue(
    target_profile: Option<String>,
    rescue_profile: Option<String>,
    issue_ids: Option<Vec<String>>,
) -> Result<RescuePrimaryRepairResult, String> {
    timed_async!("repair_primary_via_rescue", {
        let target_label = normalize_profile_name(target_profile.as_deref(), "primary");
        let rescue_label = normalize_profile_name(rescue_profile.as_deref(), "rescue");
        let requested_issue_count = issue_ids.as_ref().map_or(0, Vec::len);
        crate::logging::log_helper(&format!(
            "[local] repair_primary_via_rescue start target={} rescue={} requested_issues={}",
            target_label, rescue_label, requested_issue_count
        ));
        let result = tauri::async_runtime::spawn_blocking(move || {
            let target_profile = normalize_profile_name(target_profile.as_deref(), "primary");
            let rescue_profile = normalize_profile_name(rescue_profile.as_deref(), "rescue");
            repair_primary_via_rescue_local(
                &target_profile,
                &rescue_profile,
                issue_ids.unwrap_or_default(),
            )
        })
        .await
        .map_err(|e| e.to_string())?;

        match &result {
        Ok(summary) => crate::logging::log_helper(&format!(
            "[local] repair_primary_via_rescue success target={} rescue={} applied={} failed={} skipped={}",
            summary.target_profile,
            summary.rescue_profile,
            summary.applied_issue_ids.len(),
            summary.failed_issue_ids.len(),
            summary.skipped_issue_ids.len()
        )),
        Err(error) => crate::logging::log_helper(&format!(
            "[local] repair_primary_via_rescue failed target={} rescue={} error={}",
            target_label, rescue_label, error
        )),
    }

        result
    })
}
