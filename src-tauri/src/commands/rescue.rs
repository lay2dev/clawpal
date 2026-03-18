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

// --- Internal rescue helpers (extracted from mod.rs) ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RescueBotAction {
    Set,
    Activate,
    Status,
    Deactivate,
    Unset,
}

impl RescueBotAction {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "set" | "configure" => Ok(Self::Set),
            "activate" | "start" => Ok(Self::Activate),
            "status" => Ok(Self::Status),
            "deactivate" | "stop" => Ok(Self::Deactivate),
            "unset" | "remove" | "delete" => Ok(Self::Unset),
            _ => Err("action must be one of: set, activate, status, deactivate, unset".into()),
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Activate => "activate",
            Self::Status => "status",
            Self::Deactivate => "deactivate",
            Self::Unset => "unset",
        }
    }
}

pub(crate) fn normalize_profile_name(raw: Option<&str>, fallback: &str) -> String {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub(crate) fn build_profile_command(profile: &str, args: &[&str]) -> Vec<String> {
    let mut command = Vec::new();
    if !profile.eq_ignore_ascii_case("primary") {
        command.extend(["--profile".to_string(), profile.to_string()]);
    }
    command.extend(args.iter().map(|item| (*item).to_string()));
    command
}

pub(crate) fn build_gateway_status_command(profile: &str, use_probe: bool) -> Vec<String> {
    if use_probe {
        build_profile_command(profile, &["gateway", "status", "--json"])
    } else {
        build_profile_command(profile, &["gateway", "status", "--no-probe", "--json"])
    }
}

pub(crate) fn command_detail(output: &OpenclawCommandOutput) -> String {
    clawpal_core::doctor::command_output_detail(&output.stderr, &output.stdout)
}

pub(crate) fn gateway_output_ok(output: &OpenclawCommandOutput) -> bool {
    clawpal_core::doctor::gateway_output_ok(output.exit_code, &output.stdout, &output.stderr)
}

pub(crate) fn gateway_output_detail(output: &OpenclawCommandOutput) -> String {
    clawpal_core::doctor::gateway_output_detail(output.exit_code, &output.stdout, &output.stderr)
        .unwrap_or_else(|| command_detail(output))
}

pub(crate) fn infer_rescue_bot_runtime_state(
    configured: bool,
    status_output: Option<&OpenclawCommandOutput>,
    status_error: Option<&str>,
) -> String {
    if status_error.is_some() {
        return "error".into();
    }
    if !configured {
        return "unconfigured".into();
    }
    let Some(output) = status_output else {
        return "configured_inactive".into();
    };
    if gateway_output_ok(output) {
        return "active".into();
    }
    if let Some(value) = clawpal_core::doctor::parse_json_loose(&output.stdout)
        .or_else(|| clawpal_core::doctor::parse_json_loose(&output.stderr))
    {
        let running = value
            .get("running")
            .and_then(Value::as_bool)
            .or_else(|| value.pointer("/gateway/running").and_then(Value::as_bool));
        let healthy = value
            .get("healthy")
            .and_then(Value::as_bool)
            .or_else(|| value.pointer("/health/ok").and_then(Value::as_bool))
            .or_else(|| value.pointer("/health/healthy").and_then(Value::as_bool));
        if matches!(running, Some(false)) || matches!(healthy, Some(false)) {
            return "configured_inactive".into();
        }
    }
    let details = format!("{}\n{}", output.stderr, output.stdout).to_ascii_lowercase();
    if details.contains("not running")
        || details.contains("already stopped")
        || details.contains("not installed")
        || details.contains("not found")
        || details.contains("is not running")
        || details.contains("isn't running")
        || details.contains("\"running\":false")
        || details.contains("\"healthy\":false")
        || details.contains("\"ok\":false")
        || details.contains("inactive")
        || details.contains("stopped")
    {
        return "configured_inactive".into();
    }
    "error".into()
}

pub(crate) fn rescue_section_order() -> [&'static str; 5] {
    ["gateway", "models", "tools", "agents", "channels"]
}

pub(crate) fn rescue_section_title(key: &str) -> &'static str {
    match key {
        "gateway" => "Gateway",
        "models" => "Models",
        "tools" => "Tools",
        "agents" => "Agents",
        "channels" => "Channels",
        _ => "Recovery",
    }
}

pub(crate) fn rescue_section_docs_url(key: &str) -> &'static str {
    match key {
        "gateway" => "https://docs.openclaw.ai/gateway/security/index",
        "models" => "https://docs.openclaw.ai/models",
        "tools" => "https://docs.openclaw.ai/tools",
        "agents" => "https://docs.openclaw.ai/agents",
        "channels" => "https://docs.openclaw.ai/channels",
        _ => "https://docs.openclaw.ai/",
    }
}

pub(crate) fn section_item_status_from_issue(issue: &RescuePrimaryIssue) -> String {
    match issue.severity.as_str() {
        "error" => "error".into(),
        "warn" => "warn".into(),
        "info" => "info".into(),
        _ => "warn".into(),
    }
}

pub(crate) fn classify_rescue_check_section(
    check: &RescuePrimaryCheckItem,
) -> Option<&'static str> {
    let id = check.id.to_ascii_lowercase();
    if id.contains("gateway") || id.contains("rescue.profile") || id == "field.port" {
        return Some("gateway");
    }
    if id.contains("model") || id.contains("provider") || id.contains("auth") {
        return Some("models");
    }
    if id.contains("tool") || id.contains("allowlist") || id.contains("sandbox") {
        return Some("tools");
    }
    if id.contains("agent") || id.contains("workspace") {
        return Some("agents");
    }
    if id.contains("channel") || id.contains("discord") || id.contains("group") {
        return Some("channels");
    }
    None
}

pub(crate) fn classify_rescue_issue_section(issue: &RescuePrimaryIssue) -> &'static str {
    let haystack = format!(
        "{} {} {} {} {}",
        issue.id,
        issue.code,
        issue.message,
        issue.fix_hint.clone().unwrap_or_default(),
        issue.source
    )
    .to_ascii_lowercase();
    if issue.source == "rescue"
        || haystack.contains("gateway")
        || haystack.contains("port")
        || haystack.contains("proxy")
        || haystack.contains("security")
    {
        return "gateway";
    }
    if haystack.contains("tool")
        || haystack.contains("allowlist")
        || haystack.contains("sandbox")
        || haystack.contains("approval")
        || haystack.contains("permission")
        || haystack.contains("policy")
    {
        return "tools";
    }
    if haystack.contains("channel")
        || haystack.contains("discord")
        || haystack.contains("guild")
        || haystack.contains("allowfrom")
        || haystack.contains("groupallowfrom")
        || haystack.contains("grouppolicy")
        || haystack.contains("mention")
    {
        return "channels";
    }
    if haystack.contains("agent") || haystack.contains("workspace") || haystack.contains("session")
    {
        return "agents";
    }
    if haystack.contains("model")
        || haystack.contains("provider")
        || haystack.contains("auth")
        || haystack.contains("token")
        || haystack.contains("api key")
        || haystack.contains("apikey")
        || haystack.contains("oauth")
        || haystack.contains("base url")
    {
        return "models";
    }
    "gateway"
}

pub(crate) fn has_unreadable_primary_config_issue(issues: &[RescuePrimaryIssue]) -> bool {
    issues
        .iter()
        .any(|issue| issue.code == "primary.config.unreadable")
}

pub(crate) fn config_item(
    id: &str,
    label: &str,
    status: &str,
    detail: String,
) -> RescuePrimarySectionItem {
    RescuePrimarySectionItem {
        id: id.to_string(),
        label: label.to_string(),
        status: status.to_string(),
        detail,
        auto_fixable: false,
        issue_id: None,
    }
}

pub(crate) fn build_rescue_primary_sections(
    config: Option<&Value>,
    checks: &[RescuePrimaryCheckItem],
    issues: &[RescuePrimaryIssue],
) -> Vec<RescuePrimarySectionResult> {
    let mut grouped_items = BTreeMap::<String, Vec<RescuePrimarySectionItem>>::new();
    for key in rescue_section_order() {
        grouped_items.insert(key.to_string(), Vec::new());
    }

    if let Some(cfg) = config {
        let gateway_port = cfg
            .pointer("/gateway/port")
            .and_then(Value::as_u64)
            .map(|port| port.to_string());
        grouped_items
            .get_mut("gateway")
            .expect("gateway section must exist")
            .push(config_item(
                "gateway.config.port",
                "Gateway port",
                if gateway_port.is_some() { "ok" } else { "warn" },
                gateway_port
                    .map(|port| format!("Configured primary gateway port: {port}"))
                    .unwrap_or_else(|| "Gateway port is not explicitly configured".into()),
            ));

        let providers = cfg
            .pointer("/models/providers")
            .and_then(Value::as_object)
            .map(|providers| providers.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        grouped_items
            .get_mut("models")
            .expect("models section must exist")
            .push(config_item(
                "models.providers",
                "Provider configuration",
                if providers.is_empty() { "warn" } else { "ok" },
                if providers.is_empty() {
                    "No model providers are configured".into()
                } else {
                    format!("Configured providers: {}", providers.join(", "))
                },
            ));
        let default_model = cfg
            .pointer("/agents/defaults/model")
            .or_else(|| cfg.pointer("/agents/default/model"))
            .and_then(read_model_value);
        grouped_items
            .get_mut("models")
            .expect("models section must exist")
            .push(config_item(
                "models.defaults.primary",
                "Primary model binding",
                if default_model.is_some() {
                    "ok"
                } else {
                    "warn"
                },
                default_model
                    .map(|model| format!("Primary model resolves to {model}"))
                    .unwrap_or_else(|| "No default model binding is configured".into()),
            ));

        let tools = cfg.pointer("/tools").and_then(Value::as_object);
        grouped_items
            .get_mut("tools")
            .expect("tools section must exist")
            .push(config_item(
                "tools.config.surface",
                "Tooling surface",
                if tools.is_some() { "ok" } else { "inactive" },
                tools
                    .map(|tool_cfg| {
                        let keys = tool_cfg.keys().cloned().collect::<Vec<_>>();
                        if keys.is_empty() {
                            "Tools config exists but has no explicit controls".into()
                        } else {
                            format!("Configured tool controls: {}", keys.join(", "))
                        }
                    })
                    .unwrap_or_else(|| "No explicit tools configuration found".into()),
            ));

        let agent_count = cfg
            .pointer("/agents/list")
            .and_then(Value::as_array)
            .map(|agents| agents.len())
            .unwrap_or(0);
        grouped_items
            .get_mut("agents")
            .expect("agents section must exist")
            .push(config_item(
                "agents.config.count",
                "Agent definitions",
                if agent_count > 0 { "ok" } else { "warn" },
                if agent_count > 0 {
                    format!("Configured agents: {agent_count}")
                } else {
                    "No explicit agents.list entries were found".into()
                },
            ));

        let channel_nodes = collect_channel_nodes(cfg);
        let channel_kinds = channel_nodes
            .iter()
            .filter_map(|node| node.channel_type.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        grouped_items
            .get_mut("channels")
            .expect("channels section must exist")
            .push(config_item(
                "channels.config.count",
                "Configured channel surfaces",
                if channel_nodes.is_empty() {
                    "inactive"
                } else {
                    "ok"
                },
                if channel_nodes.is_empty() {
                    "No channels are configured".into()
                } else {
                    format!(
                        "Configured channel nodes: {} ({})",
                        channel_nodes.len(),
                        channel_kinds.join(", ")
                    )
                },
            ));
    } else {
        for key in rescue_section_order() {
            grouped_items
                .get_mut(key)
                .expect("section must exist")
                .push(config_item(
                    &format!("{key}.config.unavailable"),
                    "Configuration unavailable",
                    if key == "gateway" { "warn" } else { "inactive" },
                    "Configuration could not be read for this target".into(),
                ));
        }
    }

    for check in checks {
        let Some(section_key) = classify_rescue_check_section(check) else {
            continue;
        };
        grouped_items
            .get_mut(section_key)
            .expect("section must exist")
            .push(RescuePrimarySectionItem {
                id: check.id.clone(),
                label: check.title.clone(),
                status: if check.ok { "ok".into() } else { "warn".into() },
                detail: check.detail.clone(),
                auto_fixable: false,
                issue_id: None,
            });
    }

    for issue in issues {
        let section_key = classify_rescue_issue_section(issue);
        grouped_items
            .get_mut(section_key)
            .expect("section must exist")
            .push(RescuePrimarySectionItem {
                id: issue.id.clone(),
                label: issue.message.clone(),
                status: section_item_status_from_issue(issue),
                detail: issue.fix_hint.clone().unwrap_or_default(),
                auto_fixable: issue.auto_fixable && issue.source == "primary",
                issue_id: Some(issue.id.clone()),
            });
    }

    rescue_section_order()
        .into_iter()
        .map(|key| {
            let items = grouped_items.remove(key).unwrap_or_default();
            let has_error = items.iter().any(|item| item.status == "error");
            let has_warn = items.iter().any(|item| item.status == "warn");
            let has_active_signal = items
                .iter()
                .any(|item| item.status != "inactive" && !item.detail.is_empty());
            let status = if has_error {
                "broken"
            } else if has_warn {
                "degraded"
            } else if has_active_signal {
                "healthy"
            } else {
                "inactive"
            };
            let issue_count = items.iter().filter(|item| item.issue_id.is_some()).count();
            let summary = match status {
                "broken" => format!(
                    "{} has {} blocking finding(s)",
                    rescue_section_title(key),
                    issue_count.max(1)
                ),
                "degraded" => format!(
                    "{} has {} recommended change(s)",
                    rescue_section_title(key),
                    issue_count.max(1)
                ),
                "healthy" => format!("{} checks look healthy", rescue_section_title(key)),
                _ => format!("{} is not configured yet", rescue_section_title(key)),
            };
            RescuePrimarySectionResult {
                key: key.to_string(),
                title: rescue_section_title(key).to_string(),
                status: status.to_string(),
                summary,
                docs_url: rescue_section_docs_url(key).to_string(),
                items,
                root_cause_hypotheses: Vec::new(),
                fix_steps: Vec::new(),
                confidence: None,
                citations: Vec::new(),
                version_awareness: None,
            }
        })
        .collect()
}

pub(crate) fn build_rescue_primary_summary(
    sections: &[RescuePrimarySectionResult],
    issues: &[RescuePrimaryIssue],
) -> RescuePrimarySummary {
    let selected_fix_issue_ids = issues
        .iter()
        .filter(|issue| {
            clawpal_core::doctor::is_repairable_primary_issue(
                &issue.source,
                &issue.id,
                issue.auto_fixable,
            )
        })
        .map(|issue| issue.id.clone())
        .collect::<Vec<_>>();
    let fixable_issue_count = selected_fix_issue_ids.len();
    let status = if sections.iter().any(|section| section.status == "broken") {
        "broken"
    } else if sections.iter().any(|section| section.status == "degraded") {
        "degraded"
    } else if sections.iter().any(|section| section.status == "healthy") {
        "healthy"
    } else {
        "inactive"
    };
    let priority_section = sections
        .iter()
        .find(|section| section.status == "broken")
        .or_else(|| sections.iter().find(|section| section.status == "degraded"))
        .or_else(|| sections.iter().find(|section| section.status == "healthy"));
    if has_unreadable_primary_config_issue(issues) && status == "degraded" {
        return RescuePrimarySummary {
            status: status.to_string(),
            headline: "Configuration needs attention".into(),
            recommended_action: if fixable_issue_count > 0 {
                format!(
                    "Apply {} optimization(s) and re-run recovery",
                    fixable_issue_count
                )
            } else {
                "Repair the OpenClaw configuration before the next check".into()
            },
            fixable_issue_count,
            selected_fix_issue_ids,
            root_cause_hypotheses: Vec::new(),
            fix_steps: Vec::new(),
            confidence: None,
            citations: Vec::new(),
            version_awareness: None,
        };
    }
    let (headline, recommended_action) = match priority_section {
        Some(section) if section.status == "broken" => (
            format!("{} needs attention first", section.title),
            if fixable_issue_count > 0 {
                format!("Apply {} fix(es) and re-run recovery", fixable_issue_count)
            } else {
                format!("Review {} findings and fix them manually", section.title)
            },
        ),
        Some(section) if section.status == "degraded" => (
            format!("{} has recommended improvements", section.title),
            if fixable_issue_count > 0 {
                format!(
                    "Apply {} optimization(s) to stabilize the target",
                    fixable_issue_count
                )
            } else {
                format!(
                    "Review {} recommendations before the next check",
                    section.title
                )
            },
        ),
        Some(section) => (
            "Primary recovery checks look healthy".into(),
            format!(
                "Keep monitoring {} and re-run checks after changes",
                section.title
            ),
        ),
        None => (
            "No recovery checks are available yet".into(),
            "Configure and activate Rescue Bot before running recovery".into(),
        ),
    };

    RescuePrimarySummary {
        status: status.to_string(),
        headline,
        recommended_action,
        fixable_issue_count,
        selected_fix_issue_ids,
        root_cause_hypotheses: Vec::new(),
        fix_steps: Vec::new(),
        confidence: None,
        citations: Vec::new(),
        version_awareness: None,
    }
}

pub(crate) fn doc_guidance_section_from_url(url: &str) -> Option<&'static str> {
    let lowered = url.to_ascii_lowercase();
    if lowered.contains("/gateway") || lowered.contains("/security") {
        return Some("gateway");
    }
    if lowered.contains("/models") {
        return Some("models");
    }
    if lowered.contains("/tools") {
        return Some("tools");
    }
    if lowered.contains("/agents") {
        return Some("agents");
    }
    if lowered.contains("/channels") {
        return Some("channels");
    }
    None
}

pub(crate) fn classify_doc_guidance_section(
    guidance: &DocGuidance,
    sections: &[RescuePrimarySectionResult],
) -> Option<&'static str> {
    for citation in &guidance.citations {
        if let Some(section) = doc_guidance_section_from_url(&citation.url) {
            return Some(section);
        }
    }
    for rule in &guidance.resolver_meta.rules_matched {
        let lowered = rule.to_ascii_lowercase();
        if lowered.contains("gateway") || lowered.contains("cron") {
            return Some("gateway");
        }
        if lowered.contains("provider") || lowered.contains("auth") || lowered.contains("model") {
            return Some("models");
        }
        if lowered.contains("tool") || lowered.contains("sandbox") || lowered.contains("allowlist")
        {
            return Some("tools");
        }
        if lowered.contains("agent") || lowered.contains("workspace") {
            return Some("agents");
        }
        if lowered.contains("channel") || lowered.contains("group") || lowered.contains("pairing") {
            return Some("channels");
        }
    }
    sections
        .iter()
        .find(|section| section.status == "broken")
        .or_else(|| sections.iter().find(|section| section.status == "degraded"))
        .map(|section| match section.key.as_str() {
            "gateway" => "gateway",
            "models" => "models",
            "tools" => "tools",
            "agents" => "agents",
            "channels" => "channels",
            _ => "gateway",
        })
}

pub(crate) fn build_doc_resolve_request(
    instance_scope: &str,
    transport: &str,
    openclaw_version: Option<String>,
    issues: &[RescuePrimaryIssue],
    config_content: String,
    gateway_status: Option<String>,
) -> DocResolveRequest {
    DocResolveRequest {
        instance_scope: instance_scope.to_string(),
        transport: transport.to_string(),
        openclaw_version,
        doctor_issues: issues
            .iter()
            .map(|issue| DocResolveIssue {
                id: issue.id.clone(),
                severity: issue.severity.clone(),
                message: issue.message.clone(),
            })
            .collect(),
        config_content,
        error_log: issues
            .iter()
            .map(|issue| format!("[{}] {}", issue.severity, issue.message))
            .collect::<Vec<_>>()
            .join("\n"),
        gateway_status,
    }
}

pub(crate) fn apply_doc_guidance_to_diagnosis(
    mut diagnosis: RescuePrimaryDiagnosisResult,
    guidance: Option<DocGuidance>,
) -> RescuePrimaryDiagnosisResult {
    let Some(guidance) = guidance else {
        return diagnosis;
    };
    if !guidance.root_cause_hypotheses.is_empty() {
        diagnosis.summary.root_cause_hypotheses = guidance.root_cause_hypotheses.clone();
    }
    if !guidance.fix_steps.is_empty() {
        diagnosis.summary.fix_steps = guidance.fix_steps.clone();
        if diagnosis.summary.status != "healthy" {
            if let Some(first_step) = guidance.fix_steps.first() {
                diagnosis.summary.recommended_action = first_step.clone();
            }
        }
    }
    if !guidance.citations.is_empty() {
        diagnosis.summary.citations = guidance.citations.clone();
    }
    diagnosis.summary.confidence = Some(guidance.confidence);
    diagnosis.summary.version_awareness = Some(guidance.version_awareness.clone());

    if let Some(section_key) = classify_doc_guidance_section(&guidance, &diagnosis.sections) {
        if let Some(section) = diagnosis
            .sections
            .iter_mut()
            .find(|section| section.key == section_key)
        {
            if !guidance.root_cause_hypotheses.is_empty() {
                section.root_cause_hypotheses = guidance.root_cause_hypotheses.clone();
            }
            if !guidance.fix_steps.is_empty() {
                section.fix_steps = guidance.fix_steps.clone();
            }
            if !guidance.citations.is_empty() {
                section.citations = guidance.citations.clone();
            }
            section.confidence = Some(guidance.confidence);
            section.version_awareness = Some(guidance.version_awareness.clone());
        }
    }

    diagnosis
}

pub(crate) fn collect_local_rescue_runtime_checks(
    config: Option<&Value>,
) -> Vec<RescuePrimaryCheckItem> {
    let mut checks = Vec::new();
    if let Ok(output) = run_openclaw_raw(&["agents", "list", "--json"]) {
        if let Some(json) = parse_json_from_openclaw_output(&output) {
            let count = count_agent_entries_from_cli_json(&json).unwrap_or(0);
            checks.push(RescuePrimaryCheckItem {
                id: "agents.runtime.count".into(),
                title: "Runtime agent inventory".into(),
                ok: count > 0,
                detail: if count > 0 {
                    format!("Detected {count} agent(s) from openclaw agents list")
                } else {
                    "No agents were detected from openclaw agents list".into()
                },
            });
        }
    }

    let paths = resolve_paths();
    if let Some(catalog) = extract_model_catalog_from_cli(&paths) {
        let provider_count = catalog.len();
        let model_count = catalog
            .iter()
            .map(|provider| provider.models.len())
            .sum::<usize>();
        checks.push(RescuePrimaryCheckItem {
            id: "models.catalog.runtime".into(),
            title: "Runtime model catalog".into(),
            ok: provider_count > 0 && model_count > 0,
            detail: format!("Discovered {provider_count} provider(s) and {model_count} model(s)"),
        });
    }

    if let Some(cfg) = config {
        let channel_nodes = collect_channel_nodes(cfg);
        checks.push(RescuePrimaryCheckItem {
            id: "channels.runtime.nodes".into(),
            title: "Configured channel nodes".into(),
            ok: !channel_nodes.is_empty(),
            detail: if channel_nodes.is_empty() {
                "No channel nodes were discovered in config".into()
            } else {
                format!("Discovered {} channel node(s)", channel_nodes.len())
            },
        });
    }

    checks
}

pub(crate) async fn collect_remote_rescue_runtime_checks(
    pool: &SshConnectionPool,
    host_id: &str,
    config: Option<&Value>,
) -> Vec<RescuePrimaryCheckItem> {
    let mut checks = Vec::new();
    if let Ok(output) = run_remote_openclaw_dynamic(
        pool,
        host_id,
        vec!["agents".into(), "list".into(), "--json".into()],
    )
    .await
    {
        if let Some(json) = parse_json_from_openclaw_output(&output) {
            let count = count_agent_entries_from_cli_json(&json).unwrap_or(0);
            checks.push(RescuePrimaryCheckItem {
                id: "agents.runtime.count".into(),
                title: "Runtime agent inventory".into(),
                ok: count > 0,
                detail: if count > 0 {
                    format!("Detected {count} agent(s) from remote openclaw agents list")
                } else {
                    "No agents were detected from remote openclaw agents list".into()
                },
            });
        }
    }

    if let Ok(output) = run_remote_openclaw_dynamic(
        pool,
        host_id,
        vec![
            "models".into(),
            "list".into(),
            "--all".into(),
            "--json".into(),
            "--no-color".into(),
        ],
    )
    .await
    {
        if let Some(catalog) = parse_model_catalog_from_cli_output(&output.stdout) {
            let provider_count = catalog.len();
            let model_count = catalog
                .iter()
                .map(|provider| provider.models.len())
                .sum::<usize>();
            checks.push(RescuePrimaryCheckItem {
                id: "models.catalog.runtime".into(),
                title: "Runtime model catalog".into(),
                ok: provider_count > 0 && model_count > 0,
                detail: format!(
                    "Discovered {provider_count} provider(s) and {model_count} model(s)"
                ),
            });
        }
    }

    if let Some(cfg) = config {
        let channel_nodes = collect_channel_nodes(cfg);
        checks.push(RescuePrimaryCheckItem {
            id: "channels.runtime.nodes".into(),
            title: "Configured channel nodes".into(),
            ok: !channel_nodes.is_empty(),
            detail: if channel_nodes.is_empty() {
                "No channel nodes were discovered in config".into()
            } else {
                format!("Discovered {} channel node(s)", channel_nodes.len())
            },
        });
    }

    checks
}

pub(crate) fn build_rescue_primary_diagnosis(
    target_profile: &str,
    rescue_profile: &str,
    rescue_configured: bool,
    rescue_port: Option<u16>,
    config: Option<&Value>,
    mut runtime_checks: Vec<RescuePrimaryCheckItem>,
    rescue_gateway_status: Option<&OpenclawCommandOutput>,
    primary_doctor_output: &OpenclawCommandOutput,
    primary_gateway_status: &OpenclawCommandOutput,
) -> RescuePrimaryDiagnosisResult {
    let mut checks = Vec::new();
    checks.append(&mut runtime_checks);
    let mut issues: Vec<clawpal_core::doctor::DoctorIssue> = Vec::new();

    checks.push(RescuePrimaryCheckItem {
        id: "rescue.profile.configured".into(),
        title: "Rescue profile configured".into(),
        ok: rescue_configured,
        detail: if rescue_configured {
            rescue_port
                .map(|port| format!("profile={rescue_profile}, port={port}"))
                .unwrap_or_else(|| format!("profile={rescue_profile}, port unknown"))
        } else {
            format!("profile={rescue_profile} not configured")
        },
    });

    if !rescue_configured {
        issues.push(clawpal_core::doctor::DoctorIssue {
            id: "rescue.profile.missing".into(),
            code: "rescue.profile.missing".into(),
            severity: "error".into(),
            message: format!("Rescue profile \"{rescue_profile}\" is not configured"),
            auto_fixable: false,
            fix_hint: Some("Activate Rescue Bot first".into()),
            source: "rescue".into(),
        });
    }

    if let Some(output) = rescue_gateway_status {
        let ok = gateway_output_ok(output);
        checks.push(RescuePrimaryCheckItem {
            id: "rescue.gateway.status".into(),
            title: "Rescue gateway status".into(),
            ok,
            detail: gateway_output_detail(output),
        });
        if !ok {
            issues.push(clawpal_core::doctor::DoctorIssue {
                id: "rescue.gateway.unhealthy".into(),
                code: "rescue.gateway.unhealthy".into(),
                severity: "warn".into(),
                message: "Rescue gateway is not healthy".into(),
                auto_fixable: false,
                fix_hint: Some("Inspect rescue gateway logs before using failover".into()),
                source: "rescue".into(),
            });
        }
    }

    let doctor_report = clawpal_core::doctor::parse_json_loose(&primary_doctor_output.stdout)
        .or_else(|| clawpal_core::doctor::parse_json_loose(&primary_doctor_output.stderr));
    let doctor_issues = doctor_report
        .as_ref()
        .map(|report| clawpal_core::doctor::parse_doctor_issues(report, "primary"))
        .unwrap_or_default();
    let doctor_issue_count = doctor_issues.len();
    let doctor_score = doctor_report
        .as_ref()
        .and_then(|report| report.get("score"))
        .and_then(Value::as_i64);
    let doctor_ok_from_report = doctor_report
        .as_ref()
        .and_then(|report| report.get("ok"))
        .and_then(Value::as_bool)
        .unwrap_or(primary_doctor_output.exit_code == 0);
    let doctor_has_error = doctor_issues.iter().any(|issue| issue.severity == "error");
    let doctor_check_ok = doctor_ok_from_report && !doctor_has_error;

    let doctor_detail = if let Some(score) = doctor_score {
        format!("score={score}, issues={doctor_issue_count}")
    } else {
        command_detail(primary_doctor_output)
    };
    checks.push(RescuePrimaryCheckItem {
        id: "primary.doctor".into(),
        title: "Primary doctor report".into(),
        ok: doctor_check_ok,
        detail: doctor_detail,
    });

    if doctor_report.is_none() && primary_doctor_output.exit_code != 0 {
        issues.push(clawpal_core::doctor::DoctorIssue {
            id: "primary.doctor.failed".into(),
            code: "primary.doctor.failed".into(),
            severity: "error".into(),
            message: "Primary doctor command failed".into(),
            auto_fixable: false,
            fix_hint: Some(
                "Review doctor output in this check and open gateway logs for details".into(),
            ),
            source: "primary".into(),
        });
    }
    issues.extend(doctor_issues);

    let primary_gateway_ok = gateway_output_ok(primary_gateway_status);
    checks.push(RescuePrimaryCheckItem {
        id: "primary.gateway.status".into(),
        title: "Primary gateway status".into(),
        ok: primary_gateway_ok,
        detail: gateway_output_detail(primary_gateway_status),
    });
    if config.is_none() {
        issues.push(clawpal_core::doctor::DoctorIssue {
            id: "primary.config.unreadable".into(),
            code: "primary.config.unreadable".into(),
            severity: if primary_gateway_ok {
                "warn".into()
            } else {
                "error".into()
            },
            message: "Primary configuration could not be read".into(),
            auto_fixable: false,
            fix_hint: Some(
                "Repair openclaw.json parsing errors and re-run the primary recovery check".into(),
            ),
            source: "primary".into(),
        });
    }
    if !primary_gateway_ok {
        issues.push(clawpal_core::doctor::DoctorIssue {
            id: "primary.gateway.unhealthy".into(),
            code: "primary.gateway.unhealthy".into(),
            severity: "error".into(),
            message: "Primary gateway is not healthy".into(),
            auto_fixable: true,
            fix_hint: Some(
                "Restart primary gateway and inspect gateway logs if it stays unhealthy".into(),
            ),
            source: "primary".into(),
        });
    }

    clawpal_core::doctor::dedupe_doctor_issues(&mut issues);
    let status = clawpal_core::doctor::classify_doctor_issue_status(&issues);
    let issues: Vec<RescuePrimaryIssue> = issues
        .into_iter()
        .map(|issue| RescuePrimaryIssue {
            id: issue.id,
            code: issue.code,
            severity: issue.severity,
            message: issue.message,
            auto_fixable: issue.auto_fixable,
            fix_hint: issue.fix_hint,
            source: issue.source,
        })
        .collect();
    let sections = build_rescue_primary_sections(config, &checks, &issues);
    let summary = build_rescue_primary_summary(&sections, &issues);

    RescuePrimaryDiagnosisResult {
        status,
        checked_at: format_timestamp_from_unix(unix_timestamp_secs()),
        target_profile: target_profile.to_string(),
        rescue_profile: rescue_profile.to_string(),
        rescue_configured,
        rescue_port,
        summary,
        sections,
        checks,
        issues,
    }
}

pub(crate) fn diagnose_primary_via_rescue_local(
    target_profile: &str,
    rescue_profile: &str,
) -> Result<RescuePrimaryDiagnosisResult, String> {
    let paths = resolve_paths();
    let config = read_openclaw_config(&paths).ok();
    let config_content = fs::read_to_string(&paths.config_path)
        .ok()
        .and_then(|raw| {
            clawpal_core::config::parse_and_normalize_config(&raw)
                .ok()
                .map(|(_, normalized)| normalized)
        })
        .or_else(|| {
            config
                .as_ref()
                .and_then(|cfg| serde_json::to_string_pretty(cfg).ok())
        })
        .unwrap_or_default();
    let (rescue_configured, rescue_port) = resolve_local_rescue_profile_state(rescue_profile)?;
    let rescue_gateway_status = if rescue_configured {
        let command = build_gateway_status_command(rescue_profile, false);
        Some(run_openclaw_dynamic(&command)?)
    } else {
        None
    };
    let primary_doctor_output = run_local_primary_doctor_with_fallback(target_profile)?;
    let primary_gateway_command = build_gateway_status_command(target_profile, true);
    let primary_gateway_output = run_openclaw_dynamic(&primary_gateway_command)?;
    let runtime_checks = collect_local_rescue_runtime_checks(config.as_ref());

    let diagnosis = build_rescue_primary_diagnosis(
        target_profile,
        rescue_profile,
        rescue_configured,
        rescue_port,
        config.as_ref(),
        runtime_checks,
        rescue_gateway_status.as_ref(),
        &primary_doctor_output,
        &primary_gateway_output,
    );
    let doc_request = build_doc_resolve_request(
        "local",
        "local",
        Some(resolve_openclaw_version()),
        &diagnosis.issues,
        config_content,
        Some(gateway_output_detail(&primary_gateway_output)),
    );
    let guidance = tauri::async_runtime::block_on(resolve_local_doc_guidance(&doc_request, &paths));

    Ok(apply_doc_guidance_to_diagnosis(diagnosis, Some(guidance)))
}

pub(crate) async fn diagnose_primary_via_rescue_remote(
    pool: &SshConnectionPool,
    host_id: &str,
    target_profile: &str,
    rescue_profile: &str,
) -> Result<RescuePrimaryDiagnosisResult, String> {
    let remote_config = remote_read_openclaw_config_text_and_json(pool, host_id)
        .await
        .ok();
    let config_content = remote_config
        .as_ref()
        .map(|(_, normalized, _)| normalized.clone())
        .unwrap_or_default();
    let config = remote_config.as_ref().map(|(_, _, cfg)| cfg.clone());
    let (rescue_configured, rescue_port) =
        resolve_remote_rescue_profile_state(pool, host_id, rescue_profile).await?;
    let rescue_gateway_status = if rescue_configured {
        let command = build_gateway_status_command(rescue_profile, false);
        Some(run_remote_openclaw_dynamic(pool, host_id, command).await?)
    } else {
        None
    };
    let primary_doctor_output =
        run_remote_primary_doctor_with_fallback(pool, host_id, target_profile).await?;
    let primary_gateway_command = build_gateway_status_command(target_profile, true);
    let primary_gateway_output =
        run_remote_openclaw_dynamic(pool, host_id, primary_gateway_command).await?;
    let runtime_checks = collect_remote_rescue_runtime_checks(pool, host_id, config.as_ref()).await;

    let diagnosis = build_rescue_primary_diagnosis(
        target_profile,
        rescue_profile,
        rescue_configured,
        rescue_port,
        config.as_ref(),
        runtime_checks,
        rescue_gateway_status.as_ref(),
        &primary_doctor_output,
        &primary_gateway_output,
    );
    let remote_version = pool
        .exec_login(host_id, "openclaw --version 2>/dev/null || true")
        .await
        .ok()
        .map(|output| output.stdout.trim().to_string())
        .filter(|value| !value.is_empty());
    let doc_request = build_doc_resolve_request(
        host_id,
        "remote_ssh",
        remote_version,
        &diagnosis.issues,
        config_content,
        Some(gateway_output_detail(&primary_gateway_output)),
    );
    let guidance = resolve_remote_doc_guidance(pool, host_id, &doc_request, &resolve_paths()).await;

    Ok(apply_doc_guidance_to_diagnosis(diagnosis, Some(guidance)))
}

pub(crate) fn collect_repairable_primary_issue_ids(
    diagnosis: &RescuePrimaryDiagnosisResult,
    requested_ids: &[String],
) -> (Vec<String>, Vec<String>) {
    let issues: Vec<clawpal_core::doctor::DoctorIssue> = diagnosis
        .issues
        .iter()
        .map(|issue| clawpal_core::doctor::DoctorIssue {
            id: issue.id.clone(),
            code: issue.code.clone(),
            severity: issue.severity.clone(),
            message: issue.message.clone(),
            auto_fixable: issue.auto_fixable,
            fix_hint: issue.fix_hint.clone(),
            source: issue.source.clone(),
        })
        .collect();
    clawpal_core::doctor::collect_repairable_primary_issue_ids(&issues, requested_ids)
}

pub(crate) fn build_primary_issue_fix_command(
    target_profile: &str,
    issue_id: &str,
) -> Option<(String, Vec<String>)> {
    let (title, tail) = clawpal_core::doctor::build_primary_issue_fix_tail(issue_id)?;
    let tail_refs: Vec<&str> = tail.iter().map(String::as_str).collect();
    Some((title, build_profile_command(target_profile, &tail_refs)))
}

pub(crate) fn build_primary_doctor_fix_command(target_profile: &str) -> Vec<String> {
    build_profile_command(target_profile, &["doctor", "--fix", "--yes"])
}

pub(crate) fn should_run_primary_doctor_fix(diagnosis: &RescuePrimaryDiagnosisResult) -> bool {
    if diagnosis.status != "healthy" {
        return true;
    }

    diagnosis
        .sections
        .iter()
        .any(|section| section.status != "healthy")
}

pub(crate) fn should_refresh_rescue_helper_permissions(
    diagnosis: &RescuePrimaryDiagnosisResult,
    selected_issue_ids: &[String],
) -> bool {
    let selected = selected_issue_ids.iter().cloned().collect::<HashSet<_>>();
    diagnosis.issues.iter().any(|issue| {
        (selected.is_empty() || selected.contains(&issue.id))
            && clawpal_core::doctor::is_primary_rescue_permission_issue(
                &issue.source,
                &issue.id,
                &issue.code,
                &issue.message,
                issue.fix_hint.as_deref(),
            )
    })
}

pub(crate) fn build_step_detail(command: &[String], output: &OpenclawCommandOutput) -> String {
    if output.exit_code == 0 {
        return command_detail(output);
    }
    command_failure_message(command, output)
}

pub(crate) fn run_local_gateway_restart_with_fallback(
    profile: &str,
    steps: &mut Vec<RescuePrimaryRepairStep>,
    id_prefix: &str,
    title_prefix: &str,
) -> Result<bool, String> {
    let restart_command = build_profile_command(profile, &["gateway", "restart"]);
    let restart_output = run_openclaw_dynamic(&restart_command)?;
    let restart_ok = restart_output.exit_code == 0;
    steps.push(RescuePrimaryRepairStep {
        id: format!("{id_prefix}.restart"),
        title: format!("Restart {title_prefix}"),
        ok: restart_ok,
        detail: build_step_detail(&restart_command, &restart_output),
        command: Some(restart_command.clone()),
    });
    if restart_ok {
        return Ok(true);
    }

    if !is_gateway_restart_timeout(&restart_output) {
        return Ok(false);
    }

    let stop_command = build_profile_command(profile, &["gateway", "stop"]);
    let stop_output = run_openclaw_dynamic(&stop_command)?;
    steps.push(RescuePrimaryRepairStep {
        id: format!("{id_prefix}.stop"),
        title: format!("Stop {title_prefix} (restart fallback)"),
        ok: stop_output.exit_code == 0,
        detail: build_step_detail(&stop_command, &stop_output),
        command: Some(stop_command),
    });

    let start_command = build_profile_command(profile, &["gateway", "start"]);
    let start_output = run_openclaw_dynamic(&start_command)?;
    let start_ok = start_output.exit_code == 0;
    steps.push(RescuePrimaryRepairStep {
        id: format!("{id_prefix}.start"),
        title: format!("Start {title_prefix} (restart fallback)"),
        ok: start_ok,
        detail: build_step_detail(&start_command, &start_output),
        command: Some(start_command),
    });
    Ok(start_ok)
}

pub(crate) fn run_local_rescue_permission_refresh(
    rescue_profile: &str,
    steps: &mut Vec<RescuePrimaryRepairStep>,
) -> Result<(), String> {
    for (index, command) in
        clawpal_core::doctor::build_rescue_permission_baseline_commands(rescue_profile)
            .into_iter()
            .enumerate()
    {
        let output = run_openclaw_dynamic(&command)?;
        steps.push(RescuePrimaryRepairStep {
            id: format!("rescue.permissions.{}", index + 1),
            title: "Update recovery helper permissions".into(),
            ok: output.exit_code == 0,
            detail: build_step_detail(&command, &output),
            command: Some(command),
        });
    }
    let _ = run_local_gateway_restart_with_fallback(
        rescue_profile,
        steps,
        "rescue.gateway",
        "recovery helper",
    )?;
    Ok(())
}

pub(crate) fn run_local_primary_doctor_fix(
    profile: &str,
    steps: &mut Vec<RescuePrimaryRepairStep>,
) -> Result<bool, String> {
    let command = build_primary_doctor_fix_command(profile);
    let output = run_openclaw_dynamic(&command)?;
    let ok = output.exit_code == 0;
    steps.push(RescuePrimaryRepairStep {
        id: "primary.doctor.fix".into(),
        title: "Run openclaw doctor --fix".into(),
        ok,
        detail: build_step_detail(&command, &output),
        command: Some(command),
    });
    Ok(ok)
}

pub(crate) async fn run_remote_gateway_restart_with_fallback(
    pool: &SshConnectionPool,
    host_id: &str,
    profile: &str,
    steps: &mut Vec<RescuePrimaryRepairStep>,
    id_prefix: &str,
    title_prefix: &str,
) -> Result<bool, String> {
    let restart_command = build_profile_command(profile, &["gateway", "restart"]);
    let restart_output =
        run_remote_openclaw_dynamic(pool, host_id, restart_command.clone()).await?;
    let restart_ok = restart_output.exit_code == 0;
    steps.push(RescuePrimaryRepairStep {
        id: format!("{id_prefix}.restart"),
        title: format!("Restart {title_prefix}"),
        ok: restart_ok,
        detail: build_step_detail(&restart_command, &restart_output),
        command: Some(restart_command.clone()),
    });
    if restart_ok {
        return Ok(true);
    }

    if !is_gateway_restart_timeout(&restart_output) {
        return Ok(false);
    }

    let stop_command = build_profile_command(profile, &["gateway", "stop"]);
    let stop_output = run_remote_openclaw_dynamic(pool, host_id, stop_command.clone()).await?;
    steps.push(RescuePrimaryRepairStep {
        id: format!("{id_prefix}.stop"),
        title: format!("Stop {title_prefix} (restart fallback)"),
        ok: stop_output.exit_code == 0,
        detail: build_step_detail(&stop_command, &stop_output),
        command: Some(stop_command),
    });

    let start_command = build_profile_command(profile, &["gateway", "start"]);
    let start_output = run_remote_openclaw_dynamic(pool, host_id, start_command.clone()).await?;
    let start_ok = start_output.exit_code == 0;
    steps.push(RescuePrimaryRepairStep {
        id: format!("{id_prefix}.start"),
        title: format!("Start {title_prefix} (restart fallback)"),
        ok: start_ok,
        detail: build_step_detail(&start_command, &start_output),
        command: Some(start_command),
    });
    Ok(start_ok)
}

pub(crate) async fn run_remote_rescue_permission_refresh(
    pool: &SshConnectionPool,
    host_id: &str,
    rescue_profile: &str,
    steps: &mut Vec<RescuePrimaryRepairStep>,
) -> Result<(), String> {
    for (index, command) in
        clawpal_core::doctor::build_rescue_permission_baseline_commands(rescue_profile)
            .into_iter()
            .enumerate()
    {
        let output = run_remote_openclaw_dynamic(pool, host_id, command.clone()).await?;
        steps.push(RescuePrimaryRepairStep {
            id: format!("rescue.permissions.{}", index + 1),
            title: "Update recovery helper permissions".into(),
            ok: output.exit_code == 0,
            detail: build_step_detail(&command, &output),
            command: Some(command),
        });
    }
    let _ = run_remote_gateway_restart_with_fallback(
        pool,
        host_id,
        rescue_profile,
        steps,
        "rescue.gateway",
        "recovery helper",
    )
    .await?;
    Ok(())
}

pub(crate) async fn run_remote_primary_doctor_fix(
    pool: &SshConnectionPool,
    host_id: &str,
    profile: &str,
    steps: &mut Vec<RescuePrimaryRepairStep>,
) -> Result<bool, String> {
    let command = build_primary_doctor_fix_command(profile);
    let output = run_remote_openclaw_dynamic(pool, host_id, command.clone()).await?;
    let ok = output.exit_code == 0;
    steps.push(RescuePrimaryRepairStep {
        id: "primary.doctor.fix".into(),
        title: "Run openclaw doctor --fix".into(),
        ok,
        detail: build_step_detail(&command, &output),
        command: Some(command),
    });
    Ok(ok)
}

pub(crate) fn repair_primary_via_rescue_local(
    target_profile: &str,
    rescue_profile: &str,
    issue_ids: Vec<String>,
) -> Result<RescuePrimaryRepairResult, String> {
    let attempted_at = format_timestamp_from_unix(unix_timestamp_secs());
    let before = diagnose_primary_via_rescue_local(target_profile, rescue_profile)?;
    let (selected_issue_ids, skipped_issue_ids) =
        collect_repairable_primary_issue_ids(&before, &issue_ids);
    let mut applied_issue_ids = Vec::new();
    let mut failed_issue_ids = Vec::new();
    let mut deferred_issue_ids = Vec::new();
    let mut steps = Vec::new();
    let should_run_doctor_fix = should_run_primary_doctor_fix(&before);
    let should_refresh_rescue_permissions =
        should_refresh_rescue_helper_permissions(&before, &selected_issue_ids);

    if !before.rescue_configured {
        steps.push(RescuePrimaryRepairStep {
            id: "precheck.rescue_configured".into(),
            title: "Rescue profile availability".into(),
            ok: false,
            detail: format!(
                "Rescue profile \"{}\" is not configured; activate it before repair",
                before.rescue_profile
            ),
            command: None,
        });
        let after = before.clone();
        return Ok(RescuePrimaryRepairResult {
            status: "completed".into(),
            attempted_at,
            target_profile: target_profile.to_string(),
            rescue_profile: rescue_profile.to_string(),
            selected_issue_ids,
            applied_issue_ids,
            skipped_issue_ids,
            failed_issue_ids,
            pending_action: None,
            steps,
            before,
            after,
        });
    }

    if selected_issue_ids.is_empty() && !should_run_doctor_fix {
        steps.push(RescuePrimaryRepairStep {
            id: "repair.noop".into(),
            title: "No automatic repairs available".into(),
            ok: true,
            detail: "No primary issues were selected for repair".into(),
            command: None,
        });
    } else {
        if should_refresh_rescue_permissions {
            run_local_rescue_permission_refresh(rescue_profile, &mut steps)?;
        }
        if should_run_doctor_fix {
            let _ = run_local_primary_doctor_fix(target_profile, &mut steps)?;
        }
        let mut gateway_recovery_requested = false;
        for issue_id in &selected_issue_ids {
            if clawpal_core::doctor::is_primary_gateway_recovery_issue(issue_id) {
                gateway_recovery_requested = true;
                continue;
            }
            let Some((title, command)) = build_primary_issue_fix_command(target_profile, issue_id)
            else {
                deferred_issue_ids.push(issue_id.clone());
                steps.push(RescuePrimaryRepairStep {
                    id: format!("repair.{issue_id}"),
                    title: "Delegate issue to openclaw doctor --fix".into(),
                    ok: should_run_doctor_fix,
                    detail: if should_run_doctor_fix {
                        format!(
                            "No direct repair mapping for issue \"{issue_id}\"; relying on openclaw doctor --fix and recheck"
                        )
                    } else {
                        format!("No repair mapping for issue \"{issue_id}\"")
                    },
                    command: None,
                });
                continue;
            };
            let output = run_openclaw_dynamic(&command)?;
            let ok = output.exit_code == 0;
            steps.push(RescuePrimaryRepairStep {
                id: format!("repair.{issue_id}"),
                title,
                ok,
                detail: build_step_detail(&command, &output),
                command: Some(command),
            });
            if ok {
                applied_issue_ids.push(issue_id.clone());
            } else {
                failed_issue_ids.push(issue_id.clone());
            }
        }
        if gateway_recovery_requested || !selected_issue_ids.is_empty() || should_run_doctor_fix {
            let restart_ok = run_local_gateway_restart_with_fallback(
                target_profile,
                &mut steps,
                "primary.gateway",
                "primary gateway",
            )?;
            if gateway_recovery_requested {
                if restart_ok {
                    applied_issue_ids.push("primary.gateway.unhealthy".into());
                } else {
                    failed_issue_ids.push("primary.gateway.unhealthy".into());
                }
            } else if !restart_ok {
                failed_issue_ids.push("primary.gateway.restart".into());
            }
        }
    }

    let after = diagnose_primary_via_rescue_local(target_profile, rescue_profile)?;
    let remaining_issue_ids = after
        .issues
        .iter()
        .map(|issue| issue.id.as_str())
        .collect::<HashSet<_>>();
    for issue_id in deferred_issue_ids {
        if remaining_issue_ids.contains(issue_id.as_str()) {
            failed_issue_ids.push(issue_id);
        } else {
            applied_issue_ids.push(issue_id);
        }
    }
    Ok(RescuePrimaryRepairResult {
        status: "completed".into(),
        attempted_at,
        target_profile: target_profile.to_string(),
        rescue_profile: rescue_profile.to_string(),
        selected_issue_ids,
        applied_issue_ids,
        skipped_issue_ids,
        failed_issue_ids,
        pending_action: None,
        steps,
        before,
        after,
    })
}

pub(crate) async fn repair_primary_via_rescue_remote(
    pool: &SshConnectionPool,
    host_id: &str,
    target_profile: &str,
    rescue_profile: &str,
    issue_ids: Vec<String>,
) -> Result<RescuePrimaryRepairResult, String> {
    let attempted_at = format_timestamp_from_unix(unix_timestamp_secs());
    let before =
        diagnose_primary_via_rescue_remote(pool, host_id, target_profile, rescue_profile).await?;
    let (selected_issue_ids, skipped_issue_ids) =
        collect_repairable_primary_issue_ids(&before, &issue_ids);
    let mut applied_issue_ids = Vec::new();
    let mut failed_issue_ids = Vec::new();
    let mut deferred_issue_ids = Vec::new();
    let mut steps = Vec::new();
    let should_run_doctor_fix = should_run_primary_doctor_fix(&before);
    let should_refresh_rescue_permissions =
        should_refresh_rescue_helper_permissions(&before, &selected_issue_ids);

    if !before.rescue_configured {
        steps.push(RescuePrimaryRepairStep {
            id: "precheck.rescue_configured".into(),
            title: "Rescue profile availability".into(),
            ok: false,
            detail: format!(
                "Rescue profile \"{}\" is not configured; activate it before repair",
                before.rescue_profile
            ),
            command: None,
        });
        let after = before.clone();
        return Ok(RescuePrimaryRepairResult {
            status: "completed".into(),
            attempted_at,
            target_profile: target_profile.to_string(),
            rescue_profile: rescue_profile.to_string(),
            selected_issue_ids,
            applied_issue_ids,
            skipped_issue_ids,
            failed_issue_ids,
            pending_action: None,
            steps,
            before,
            after,
        });
    }

    if selected_issue_ids.is_empty() && !should_run_doctor_fix {
        steps.push(RescuePrimaryRepairStep {
            id: "repair.noop".into(),
            title: "No automatic repairs available".into(),
            ok: true,
            detail: "No primary issues were selected for repair".into(),
            command: None,
        });
    } else {
        if should_refresh_rescue_permissions {
            run_remote_rescue_permission_refresh(pool, host_id, rescue_profile, &mut steps).await?;
        }
        if should_run_doctor_fix {
            let _ =
                run_remote_primary_doctor_fix(pool, host_id, target_profile, &mut steps).await?;
        }
        let mut gateway_recovery_requested = false;
        for issue_id in &selected_issue_ids {
            if clawpal_core::doctor::is_primary_gateway_recovery_issue(issue_id) {
                gateway_recovery_requested = true;
                continue;
            }
            let Some((title, command)) = build_primary_issue_fix_command(target_profile, issue_id)
            else {
                deferred_issue_ids.push(issue_id.clone());
                steps.push(RescuePrimaryRepairStep {
                    id: format!("repair.{issue_id}"),
                    title: "Delegate issue to openclaw doctor --fix".into(),
                    ok: should_run_doctor_fix,
                    detail: if should_run_doctor_fix {
                        format!(
                            "No direct repair mapping for issue \"{issue_id}\"; relying on openclaw doctor --fix and recheck"
                        )
                    } else {
                        format!("No repair mapping for issue \"{issue_id}\"")
                    },
                    command: None,
                });
                continue;
            };
            let output = run_remote_openclaw_dynamic(pool, host_id, command.clone()).await?;
            let ok = output.exit_code == 0;
            steps.push(RescuePrimaryRepairStep {
                id: format!("repair.{issue_id}"),
                title,
                ok,
                detail: build_step_detail(&command, &output),
                command: Some(command),
            });
            if ok {
                applied_issue_ids.push(issue_id.clone());
            } else {
                failed_issue_ids.push(issue_id.clone());
            }
        }
        if gateway_recovery_requested || !selected_issue_ids.is_empty() || should_run_doctor_fix {
            let restart_ok = run_remote_gateway_restart_with_fallback(
                pool,
                host_id,
                target_profile,
                &mut steps,
                "primary.gateway",
                "primary gateway",
            )
            .await?;
            if gateway_recovery_requested {
                if restart_ok {
                    applied_issue_ids.push("primary.gateway.unhealthy".into());
                } else {
                    failed_issue_ids.push("primary.gateway.unhealthy".into());
                }
            } else if !restart_ok {
                failed_issue_ids.push("primary.gateway.restart".into());
            }
        }
    }

    let after =
        diagnose_primary_via_rescue_remote(pool, host_id, target_profile, rescue_profile).await?;
    let remaining_issue_ids = after
        .issues
        .iter()
        .map(|issue| issue.id.as_str())
        .collect::<HashSet<_>>();
    for issue_id in deferred_issue_ids {
        if remaining_issue_ids.contains(issue_id.as_str()) {
            failed_issue_ids.push(issue_id);
        } else {
            applied_issue_ids.push(issue_id);
        }
    }
    Ok(RescuePrimaryRepairResult {
        status: "completed".into(),
        attempted_at,
        target_profile: target_profile.to_string(),
        rescue_profile: rescue_profile.to_string(),
        selected_issue_ids,
        applied_issue_ids,
        skipped_issue_ids,
        failed_issue_ids,
        pending_action: None,
        steps,
        before,
        after,
    })
}

pub(crate) fn resolve_local_rescue_profile_state(
    profile: &str,
) -> Result<(bool, Option<u16>), String> {
    let output = crate::cli_runner::run_openclaw(&[
        "--profile",
        profile,
        "config",
        "get",
        "gateway.port",
        "--json",
    ])?;
    if output.exit_code != 0 {
        return Ok((false, None));
    }
    let port = crate::cli_runner::parse_json_output(&output)
        .ok()
        .and_then(|value| clawpal_core::doctor::parse_rescue_port_value(&value));
    Ok((true, port))
}

pub(crate) fn build_rescue_bot_command_plan(
    action: RescueBotAction,
    profile: &str,
    rescue_port: u16,
    include_configure: bool,
) -> Vec<Vec<String>> {
    clawpal_core::doctor::build_rescue_bot_command_plan(
        action.as_str(),
        profile,
        rescue_port,
        include_configure,
    )
}

pub(crate) fn command_failure_message(
    command: &[String],
    output: &OpenclawCommandOutput,
) -> String {
    clawpal_core::doctor::command_failure_message(
        command,
        output.exit_code,
        &output.stderr,
        &output.stdout,
    )
}

pub(crate) fn is_gateway_restart_command(command: &[String]) -> bool {
    clawpal_core::doctor::is_gateway_restart_command(command)
}

pub(crate) fn is_gateway_restart_timeout(output: &OpenclawCommandOutput) -> bool {
    clawpal_core::doctor::gateway_restart_timeout(&output.stderr, &output.stdout)
}

pub(crate) fn is_rescue_cleanup_noop(
    action: RescueBotAction,
    command: &[String],
    output: &OpenclawCommandOutput,
) -> bool {
    clawpal_core::doctor::rescue_cleanup_noop(
        action.as_str(),
        command,
        output.exit_code,
        &output.stderr,
        &output.stdout,
    )
}

pub(crate) fn run_local_rescue_bot_command(
    command: Vec<String>,
) -> Result<RescueBotCommandResult, String> {
    let output = run_openclaw_dynamic(&command)?;
    if is_gateway_status_command_output_incompatible(&output, &command) {
        let fallback = strip_gateway_status_json_flag(&command);
        if fallback != command {
            let fallback_output = run_openclaw_dynamic(&fallback)?;
            return Ok(RescueBotCommandResult {
                command: fallback,
                output: fallback_output,
            });
        }
    }
    Ok(RescueBotCommandResult { command, output })
}

pub(crate) fn is_gateway_status_command_output_incompatible(
    output: &OpenclawCommandOutput,
    command: &[String],
) -> bool {
    if output.exit_code == 0 {
        return false;
    }
    if !command.iter().any(|arg| arg == "--json") {
        return false;
    }
    clawpal_core::doctor::doctor_json_option_unsupported(&output.stderr, &output.stdout)
}

pub(crate) fn strip_gateway_status_json_flag(command: &[String]) -> Vec<String> {
    command
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .cloned()
        .collect()
}

pub(crate) fn run_local_primary_doctor_with_fallback(
    profile: &str,
) -> Result<OpenclawCommandOutput, String> {
    let json_command = build_profile_command(profile, &["doctor", "--json", "--yes"]);
    let output = run_openclaw_dynamic(&json_command)?;
    if output.exit_code != 0
        && clawpal_core::doctor::doctor_json_option_unsupported(&output.stderr, &output.stdout)
    {
        let plain_command = build_profile_command(profile, &["doctor", "--yes"]);
        return run_openclaw_dynamic(&plain_command);
    }
    Ok(output)
}

pub(crate) fn run_local_gateway_restart_fallback(
    profile: &str,
    commands: &mut Vec<RescueBotCommandResult>,
) -> Result<(), String> {
    let stop_command = vec![
        "--profile".to_string(),
        profile.to_string(),
        "gateway".to_string(),
        "stop".to_string(),
    ];
    let stop_result = run_local_rescue_bot_command(stop_command)?;
    commands.push(stop_result);

    let start_command = vec![
        "--profile".to_string(),
        profile.to_string(),
        "gateway".to_string(),
        "start".to_string(),
    ];
    let start_result = run_local_rescue_bot_command(start_command)?;
    if start_result.output.exit_code != 0 {
        return Err(command_failure_message(
            &start_result.command,
            &start_result.output,
        ));
    }
    commands.push(start_result);
    Ok(())
}

pub(crate) async fn resolve_remote_rescue_profile_state(
    pool: &SshConnectionPool,
    host_id: &str,
    profile: &str,
) -> Result<(bool, Option<u16>), String> {
    let output = crate::cli_runner::run_openclaw_remote(
        pool,
        host_id,
        &[
            "--profile",
            profile,
            "config",
            "get",
            "gateway.port",
            "--json",
        ],
    )
    .await?;
    if output.exit_code != 0 {
        return Ok((false, None));
    }
    let port = crate::cli_runner::parse_json_output(&output)
        .ok()
        .and_then(|value| clawpal_core::doctor::parse_rescue_port_value(&value));
    Ok((true, port))
}

#[cfg(test)]
mod rescue_bot_tests {
    use super::*;

    #[test]
    fn test_suggest_rescue_port_prefers_large_gap() {
        assert_eq!(clawpal_core::doctor::suggest_rescue_port(18789), 19789);
    }

    #[test]
    fn test_ensure_rescue_port_spacing_rejects_small_gap() {
        let err = clawpal_core::doctor::ensure_rescue_port_spacing(18789, 18800).unwrap_err();
        assert!(err.contains(">= +20"));
    }

    #[test]
    fn test_build_rescue_bot_command_plan_for_activate() {
        let commands =
            build_rescue_bot_command_plan(RescueBotAction::Activate, "rescue", 19789, true);
        let expected = vec![
            vec!["--profile", "rescue", "setup"],
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "gateway.port",
                "19789",
                "--json",
            ],
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "tools.profile",
                "\"full\"",
                "--json",
            ],
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "tools.sessions.visibility",
                "\"all\"",
                "--json",
            ],
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "tools.allow",
                "[\"*\"]",
                "--json",
            ],
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "tools.exec.host",
                "\"gateway\"",
                "--json",
            ],
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "tools.exec.security",
                "\"full\"",
                "--json",
            ],
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "tools.exec.ask",
                "\"off\"",
                "--json",
            ],
            vec!["--profile", "rescue", "gateway", "stop"],
            vec!["--profile", "rescue", "gateway", "uninstall"],
            vec!["--profile", "rescue", "gateway", "install"],
            vec!["--profile", "rescue", "gateway", "start"],
            vec!["--profile", "rescue", "gateway", "status", "--json"],
        ]
        .into_iter()
        .map(|items| items.into_iter().map(String::from).collect::<Vec<_>>())
        .collect::<Vec<_>>();
        assert_eq!(commands, expected);
    }

    #[test]
    fn test_build_rescue_bot_command_plan_for_activate_without_reconfigure() {
        let commands =
            build_rescue_bot_command_plan(RescueBotAction::Activate, "rescue", 19789, false);
        let expected = vec![
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "tools.profile",
                "\"full\"",
                "--json",
            ],
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "tools.sessions.visibility",
                "\"all\"",
                "--json",
            ],
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "tools.allow",
                "[\"*\"]",
                "--json",
            ],
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "tools.exec.host",
                "\"gateway\"",
                "--json",
            ],
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "tools.exec.security",
                "\"full\"",
                "--json",
            ],
            vec![
                "--profile",
                "rescue",
                "config",
                "set",
                "tools.exec.ask",
                "\"off\"",
                "--json",
            ],
            vec!["--profile", "rescue", "gateway", "install"],
            vec!["--profile", "rescue", "gateway", "restart"],
            vec![
                "--profile",
                "rescue",
                "gateway",
                "status",
                "--no-probe",
                "--json",
            ],
        ]
        .into_iter()
        .map(|items| items.into_iter().map(String::from).collect::<Vec<_>>())
        .collect::<Vec<_>>();
        assert_eq!(commands, expected);
    }

    #[test]
    fn test_build_rescue_bot_command_plan_for_unset() {
        let commands =
            build_rescue_bot_command_plan(RescueBotAction::Unset, "rescue", 19789, false);
        let expected = vec![
            vec!["--profile", "rescue", "gateway", "stop"],
            vec!["--profile", "rescue", "gateway", "uninstall"],
            vec!["--profile", "rescue", "config", "unset", "gateway.port"],
        ]
        .into_iter()
        .map(|items| items.into_iter().map(String::from).collect::<Vec<_>>())
        .collect::<Vec<_>>();
        assert_eq!(commands, expected);
    }

    #[test]
    fn test_parse_rescue_bot_action_unset_aliases() {
        assert_eq!(
            RescueBotAction::parse("unset").unwrap(),
            RescueBotAction::Unset
        );
        assert_eq!(
            RescueBotAction::parse("remove").unwrap(),
            RescueBotAction::Unset
        );
        assert_eq!(
            RescueBotAction::parse("delete").unwrap(),
            RescueBotAction::Unset
        );
    }

    #[test]
    fn test_is_rescue_cleanup_noop_matches_stop_not_running() {
        let output = OpenclawCommandOutput {
            stdout: String::new(),
            stderr: "Gateway is not running".into(),
            exit_code: 1,
        };
        let command = vec![
            "--profile".to_string(),
            "rescue".to_string(),
            "gateway".to_string(),
            "stop".to_string(),
        ];
        assert!(is_rescue_cleanup_noop(
            RescueBotAction::Deactivate,
            &command,
            &output
        ));
    }

    #[test]
    fn test_is_rescue_cleanup_noop_matches_unset_missing_key() {
        let output = OpenclawCommandOutput {
            stdout: String::new(),
            stderr: "config key gateway.port not found".into(),
            exit_code: 1,
        };
        let command = vec![
            "--profile".to_string(),
            "rescue".to_string(),
            "config".to_string(),
            "unset".to_string(),
            "gateway.port".to_string(),
        ];
        assert!(is_rescue_cleanup_noop(
            RescueBotAction::Unset,
            &command,
            &output
        ));
    }

    #[test]
    fn test_is_gateway_restart_timeout_matches_health_check_timeout() {
        let output = OpenclawCommandOutput {
            stdout: String::new(),
            stderr: "Gateway restart timed out after 60s waiting for health checks.".into(),
            exit_code: 1,
        };
        assert!(clawpal_core::doctor::gateway_restart_timeout(
            &output.stderr,
            &output.stdout
        ));
    }

    #[test]
    fn test_is_gateway_restart_timeout_ignores_other_errors() {
        let output = OpenclawCommandOutput {
            stdout: String::new(),
            stderr: "gateway start failed: address already in use".into(),
            exit_code: 1,
        };
        assert!(!clawpal_core::doctor::gateway_restart_timeout(
            &output.stderr,
            &output.stdout
        ));
    }

    #[test]
    fn test_doctor_json_option_unsupported_matches_unknown_option() {
        let output = OpenclawCommandOutput {
            stdout: String::new(),
            stderr: "error: unknown option '--json'".into(),
            exit_code: 1,
        };
        assert!(clawpal_core::doctor::doctor_json_option_unsupported(
            &output.stderr,
            &output.stdout
        ));
    }

    #[test]
    fn test_doctor_json_option_unsupported_ignores_other_failures() {
        let output = OpenclawCommandOutput {
            stdout: String::new(),
            stderr: "doctor command failed to connect".into(),
            exit_code: 1,
        };
        assert!(!clawpal_core::doctor::doctor_json_option_unsupported(
            &output.stderr,
            &output.stdout
        ));
    }

    #[test]
    fn test_gateway_command_output_incompatible_matches_unknown_json_option() {
        let output = OpenclawCommandOutput {
            stdout: String::new(),
            stderr: "error: unknown option '--json'".into(),
            exit_code: 1,
        };
        let command = vec![
            "--profile",
            "rescue",
            "gateway",
            "status",
            "--no-probe",
            "--json",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<String>>();
        assert!(is_gateway_status_command_output_incompatible(
            &output, &command
        ));
    }

    #[test]
    fn test_rescue_config_command_output_incompatible_matches_unknown_json_option() {
        let output = OpenclawCommandOutput {
            stdout: String::new(),
            stderr: "error: unknown option '--json'".into(),
            exit_code: 1,
        };
        let command = vec![
            "--profile",
            "rescue",
            "config",
            "set",
            "tools.profile",
            "full",
            "--json",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<String>>();
        assert!(is_gateway_status_command_output_incompatible(
            &output, &command
        ));
    }

    #[test]
    fn test_strip_gateway_status_json_flag_keeps_other_args() {
        let command = vec!["gateway", "status", "--json", "--no-probe", "extra"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
        assert_eq!(
            strip_gateway_status_json_flag(&command),
            vec!["gateway", "status", "--no-probe", "extra"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_parse_doctor_issues_reads_camel_case_fields() {
        let report = serde_json::json!({
            "issues": [
                {
                    "id": "primary.test",
                    "code": "primary.test",
                    "severity": "warn",
                    "message": "test issue",
                    "autoFixable": true,
                    "fixHint": "do thing"
                }
            ]
        });
        let issues = clawpal_core::doctor::parse_doctor_issues(&report, "primary");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].id, "primary.test");
        assert_eq!(issues[0].severity, "warn");
        assert!(issues[0].auto_fixable);
        assert_eq!(issues[0].fix_hint.as_deref(), Some("do thing"));
    }

    #[test]
    fn test_extract_json_from_output_uses_trailing_balanced_payload() {
        let raw = "[plugins] warmup cache\n[warn] using fallback transport\n{\"ok\":false,\"issues\":[{\"id\":\"x\"}]}";
        let json = clawpal_core::doctor::extract_json_from_output(raw).unwrap();
        assert_eq!(json, "{\"ok\":false,\"issues\":[{\"id\":\"x\"}]}");
    }

    #[test]
    fn test_parse_json_loose_handles_leading_bracketed_logs() {
        let raw = "[plugins] warmup cache\n[warn] using fallback transport\n{\"running\":false,\"healthy\":false}";
        let parsed =
            clawpal_core::doctor::parse_json_loose(raw).expect("expected trailing JSON payload");
        assert_eq!(parsed.get("running").and_then(Value::as_bool), Some(false));
        assert_eq!(parsed.get("healthy").and_then(Value::as_bool), Some(false));
    }

    #[test]
    fn test_classify_doctor_issue_status_prioritizes_error() {
        let issues = vec![
            RescuePrimaryIssue {
                id: "a".into(),
                code: "a".into(),
                severity: "warn".into(),
                message: "warn".into(),
                auto_fixable: false,
                fix_hint: None,
                source: "primary".into(),
            },
            RescuePrimaryIssue {
                id: "b".into(),
                code: "b".into(),
                severity: "error".into(),
                message: "error".into(),
                auto_fixable: false,
                fix_hint: None,
                source: "primary".into(),
            },
        ];
        let core: Vec<clawpal_core::doctor::DoctorIssue> = issues
            .into_iter()
            .map(|issue| clawpal_core::doctor::DoctorIssue {
                id: issue.id,
                code: issue.code,
                severity: issue.severity,
                message: issue.message,
                auto_fixable: issue.auto_fixable,
                fix_hint: issue.fix_hint,
                source: issue.source,
            })
            .collect();
        assert_eq!(
            clawpal_core::doctor::classify_doctor_issue_status(&core),
            "broken"
        );
    }

    #[test]
    fn test_collect_repairable_primary_issue_ids_filters_non_primary_only() {
        let diagnosis = RescuePrimaryDiagnosisResult {
            status: "degraded".into(),
            checked_at: "2026-02-25T00:00:00Z".into(),
            target_profile: "primary".into(),
            rescue_profile: "rescue".into(),
            rescue_configured: true,
            rescue_port: Some(19789),
            summary: RescuePrimarySummary {
                status: "degraded".into(),
                headline: "Primary configuration needs attention".into(),
                recommended_action: "Review fixable issues".into(),
                fixable_issue_count: 1,
                selected_fix_issue_ids: vec!["field.agents".into()],
                root_cause_hypotheses: Vec::new(),
                fix_steps: Vec::new(),
                confidence: None,
                citations: Vec::new(),
                version_awareness: None,
            },
            sections: Vec::new(),
            checks: Vec::new(),
            issues: vec![
                RescuePrimaryIssue {
                    id: "field.agents".into(),
                    code: "required.field".into(),
                    severity: "warn".into(),
                    message: "missing agents".into(),
                    auto_fixable: true,
                    fix_hint: None,
                    source: "primary".into(),
                },
                RescuePrimaryIssue {
                    id: "field.port".into(),
                    code: "invalid.port".into(),
                    severity: "error".into(),
                    message: "port invalid".into(),
                    auto_fixable: false,
                    fix_hint: None,
                    source: "primary".into(),
                },
                RescuePrimaryIssue {
                    id: "rescue.gateway.unhealthy".into(),
                    code: "rescue.gateway.unhealthy".into(),
                    severity: "warn".into(),
                    message: "rescue unhealthy".into(),
                    auto_fixable: true,
                    fix_hint: None,
                    source: "rescue".into(),
                },
            ],
        };

        let (selected, skipped) = collect_repairable_primary_issue_ids(
            &diagnosis,
            &[
                "field.agents".into(),
                "field.port".into(),
                "rescue.gateway.unhealthy".into(),
            ],
        );
        assert_eq!(selected, vec!["field.port"]);
        assert_eq!(skipped, vec!["field.agents", "rescue.gateway.unhealthy"]);
    }

    #[test]
    fn test_build_primary_issue_fix_command_for_field_port() {
        let (_, command) = build_primary_issue_fix_command("primary", "field.port")
            .expect("field.port should have safe fix command");
        assert_eq!(
            command,
            vec!["config", "set", "gateway.port", "18789", "--json"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_build_primary_doctor_fix_command_for_profile() {
        let command = build_primary_doctor_fix_command("primary");
        assert_eq!(
            command,
            vec!["doctor", "--fix", "--yes"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_build_gateway_status_command_uses_probe_for_primary_diagnosis_only() {
        assert_eq!(
            build_gateway_status_command("primary", true),
            vec!["gateway", "status", "--json"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            build_gateway_status_command("rescue", false),
            vec![
                "--profile",
                "rescue",
                "gateway",
                "status",
                "--no-probe",
                "--json"
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_build_profile_command_omits_primary_profile_flag() {
        assert_eq!(
            build_profile_command("primary", &["doctor", "--json", "--yes"]),
            vec!["doctor", "--json", "--yes"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            build_profile_command("rescue", &["gateway", "status", "--no-probe", "--json"]),
            vec![
                "--profile",
                "rescue",
                "gateway",
                "status",
                "--no-probe",
                "--json"
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_should_run_primary_doctor_fix_for_non_healthy_sections() {
        let mut diagnosis = RescuePrimaryDiagnosisResult {
            status: "degraded".into(),
            checked_at: "2026-03-08T00:00:00Z".into(),
            target_profile: "primary".into(),
            rescue_profile: "rescue".into(),
            rescue_configured: true,
            rescue_port: Some(19789),
            summary: RescuePrimarySummary {
                status: "degraded".into(),
                headline: "Review recommendations".into(),
                recommended_action: "Review recommendations".into(),
                fixable_issue_count: 0,
                selected_fix_issue_ids: Vec::new(),
                root_cause_hypotheses: Vec::new(),
                fix_steps: Vec::new(),
                confidence: None,
                citations: Vec::new(),
                version_awareness: None,
            },
            sections: vec![
                RescuePrimarySectionResult {
                    key: "gateway".into(),
                    title: "Gateway".into(),
                    status: "healthy".into(),
                    summary: "Gateway is healthy".into(),
                    docs_url: String::new(),
                    items: Vec::new(),
                    root_cause_hypotheses: Vec::new(),
                    fix_steps: Vec::new(),
                    confidence: None,
                    citations: Vec::new(),
                    version_awareness: None,
                },
                RescuePrimarySectionResult {
                    key: "channels".into(),
                    title: "Channels".into(),
                    status: "inactive".into(),
                    summary: "Channels are inactive".into(),
                    docs_url: String::new(),
                    items: Vec::new(),
                    root_cause_hypotheses: Vec::new(),
                    fix_steps: Vec::new(),
                    confidence: None,
                    citations: Vec::new(),
                    version_awareness: None,
                },
            ],
            checks: Vec::new(),
            issues: Vec::new(),
        };

        assert!(should_run_primary_doctor_fix(&diagnosis));

        diagnosis.status = "healthy".into();
        diagnosis.summary.status = "healthy".into();
        diagnosis.sections[1].status = "degraded".into();
        assert!(should_run_primary_doctor_fix(&diagnosis));

        diagnosis.sections[1].status = "healthy".into();
        assert!(!should_run_primary_doctor_fix(&diagnosis));
    }

    #[test]
    fn test_should_refresh_rescue_helper_permissions_when_permission_issue_is_selected() {
        let diagnosis = RescuePrimaryDiagnosisResult {
            status: "degraded".into(),
            checked_at: "2026-03-08T00:00:00Z".into(),
            target_profile: "primary".into(),
            rescue_profile: "rescue".into(),
            rescue_configured: true,
            rescue_port: Some(19789),
            summary: RescuePrimarySummary {
                status: "degraded".into(),
                headline: "Tools have recommended improvements".into(),
                recommended_action: "Apply 1 optimization".into(),
                fixable_issue_count: 1,
                selected_fix_issue_ids: vec!["tools.allowlist.review".into()],
                root_cause_hypotheses: Vec::new(),
                fix_steps: Vec::new(),
                confidence: None,
                citations: Vec::new(),
                version_awareness: None,
            },
            sections: Vec::new(),
            checks: Vec::new(),
            issues: vec![RescuePrimaryIssue {
                id: "tools.allowlist.review".into(),
                code: "tools.allowlist.review".into(),
                severity: "warn".into(),
                message: "Allowlist blocks rescue helper access".into(),
                auto_fixable: true,
                fix_hint: Some("Expand tools.allow and sessions visibility".into()),
                source: "primary".into(),
            }],
        };

        assert!(should_refresh_rescue_helper_permissions(
            &diagnosis,
            &["tools.allowlist.review".into()],
        ));
    }

    #[test]
    fn test_infer_rescue_bot_runtime_state_distinguishes_profile_states() {
        let active_output = OpenclawCommandOutput {
            stdout: "{\"running\":true,\"healthy\":true}".into(),
            stderr: String::new(),
            exit_code: 0,
        };
        let inactive_output = OpenclawCommandOutput {
            stdout: String::new(),
            stderr: "Gateway is not running".into(),
            exit_code: 1,
        };
        let inactive_json_output = OpenclawCommandOutput {
            stdout: "{\"running\":false,\"healthy\":false}".into(),
            stderr: String::new(),
            exit_code: 0,
        };

        assert_eq!(
            infer_rescue_bot_runtime_state(false, None, None),
            "unconfigured"
        );
        assert_eq!(
            infer_rescue_bot_runtime_state(true, Some(&inactive_output), None),
            "configured_inactive"
        );
        assert_eq!(
            infer_rescue_bot_runtime_state(true, Some(&active_output), None),
            "active"
        );
        assert_eq!(
            infer_rescue_bot_runtime_state(true, Some(&inactive_json_output), None),
            "configured_inactive"
        );
        assert_eq!(
            infer_rescue_bot_runtime_state(true, None, Some("probe failed")),
            "error"
        );
    }

    #[test]
    fn test_build_rescue_primary_sections_and_summary_returns_global_fix_shape() {
        let cfg = serde_json::json!({
            "gateway": { "port": 18789 },
            "models": {
                "providers": {
                    "openai": { "apiKey": "sk-test" }
                }
            },
            "tools": {
                "allowlist": ["git status", "git diff"],
                "execution": { "mode": "manual" }
            },
            "agents": {
                "defaults": { "model": "openai/gpt-5" },
                "list": [{ "id": "writer", "model": "openai/gpt-5" }]
            },
            "channels": {
                "discord": {
                    "botToken": "discord-token",
                    "guilds": {
                        "guild-1": {
                            "channels": {
                                "general": { "model": "openai/gpt-5" }
                            }
                        }
                    }
                }
            }
        });
        let checks = vec![
            RescuePrimaryCheckItem {
                id: "rescue.profile.configured".into(),
                title: "Rescue profile configured".into(),
                ok: true,
                detail: "profile=rescue, port=19789".into(),
            },
            RescuePrimaryCheckItem {
                id: "primary.gateway.status".into(),
                title: "Primary gateway status".into(),
                ok: false,
                detail: "gateway not healthy".into(),
            },
        ];
        let issues = vec![
            RescuePrimaryIssue {
                id: "primary.gateway.unhealthy".into(),
                code: "primary.gateway.unhealthy".into(),
                severity: "error".into(),
                message: "Primary gateway is not healthy".into(),
                auto_fixable: false,
                fix_hint: Some("Restart primary gateway".into()),
                source: "primary".into(),
            },
            RescuePrimaryIssue {
                id: "field.agents".into(),
                code: "required.field".into(),
                severity: "warn".into(),
                message: "missing agents".into(),
                auto_fixable: true,
                fix_hint: Some("Initialize agents.defaults.model".into()),
                source: "primary".into(),
            },
            RescuePrimaryIssue {
                id: "tools.allowlist.review".into(),
                code: "tools.allowlist.review".into(),
                severity: "warn".into(),
                message: "Review tool allowlist".into(),
                auto_fixable: false,
                fix_hint: Some("Narrow tool scope".into()),
                source: "primary".into(),
            },
        ];

        let sections = build_rescue_primary_sections(Some(&cfg), &checks, &issues);
        let summary = build_rescue_primary_summary(&sections, &issues);

        let keys = sections
            .iter()
            .map(|section| section.key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec!["gateway", "models", "tools", "agents", "channels"]
        );
        assert_eq!(sections[0].status, "broken");
        assert_eq!(sections[2].status, "degraded");
        assert_eq!(sections[3].status, "degraded");
        assert_eq!(summary.status, "broken");
        assert_eq!(summary.fixable_issue_count, 1);
        assert_eq!(
            summary.selected_fix_issue_ids,
            vec!["primary.gateway.unhealthy"]
        );
        assert!(summary.headline.contains("Gateway"));
        assert!(summary.recommended_action.contains("Apply 1 fix(es)"));
    }

    #[test]
    fn test_build_rescue_primary_summary_marks_unreadable_config_as_degraded_when_gateway_is_healthy(
    ) {
        let checks = vec![RescuePrimaryCheckItem {
            id: "primary.gateway.status".into(),
            title: "Primary gateway status".into(),
            ok: true,
            detail: "running=true, healthy=true, port=18789".into(),
        }];

        let sections = build_rescue_primary_sections(None, &checks, &[]);
        let summary = build_rescue_primary_summary(&sections, &[]);

        assert_eq!(summary.status, "degraded");
        assert!(
            summary.headline.contains("Configuration")
                || summary.headline.contains("Gateway")
                || summary.headline.contains("recommended")
        );
    }

    #[test]
    fn test_build_rescue_primary_summary_marks_unreadable_config_and_gateway_down_as_broken() {
        let checks = vec![RescuePrimaryCheckItem {
            id: "primary.gateway.status".into(),
            title: "Primary gateway status".into(),
            ok: false,
            detail: "Gateway is not running".into(),
        }];
        let issues = vec![RescuePrimaryIssue {
            id: "primary.gateway.unhealthy".into(),
            code: "primary.gateway.unhealthy".into(),
            severity: "error".into(),
            message: "Primary gateway is not healthy".into(),
            auto_fixable: true,
            fix_hint: Some("Restart primary gateway".into()),
            source: "primary".into(),
        }];

        let sections = build_rescue_primary_sections(None, &checks, &issues);
        let summary = build_rescue_primary_summary(&sections, &issues);

        assert_eq!(summary.status, "broken");
        assert!(summary.headline.contains("Gateway"));
    }

    #[test]
    fn test_apply_doc_guidance_attaches_to_summary_and_matching_section() {
        let diagnosis = RescuePrimaryDiagnosisResult {
            status: "degraded".into(),
            checked_at: "2026-03-08T00:00:00Z".into(),
            target_profile: "primary".into(),
            rescue_profile: "rescue".into(),
            rescue_configured: true,
            rescue_port: Some(19789),
            summary: RescuePrimarySummary {
                status: "degraded".into(),
                headline: "Agents has recommended improvements".into(),
                recommended_action: "Review agent recommendations".into(),
                fixable_issue_count: 1,
                selected_fix_issue_ids: vec!["field.agents".into()],
                root_cause_hypotheses: Vec::new(),
                fix_steps: Vec::new(),
                confidence: None,
                citations: Vec::new(),
                version_awareness: None,
            },
            sections: vec![RescuePrimarySectionResult {
                key: "agents".into(),
                title: "Agents".into(),
                status: "degraded".into(),
                summary: "Agents has 1 recommended change".into(),
                docs_url: "https://docs.openclaw.ai/agents".into(),
                items: Vec::new(),
                root_cause_hypotheses: Vec::new(),
                fix_steps: Vec::new(),
                confidence: None,
                citations: Vec::new(),
                version_awareness: None,
            }],
            checks: Vec::new(),
            issues: vec![RescuePrimaryIssue {
                id: "field.agents".into(),
                code: "required.field".into(),
                severity: "warn".into(),
                message: "missing agents".into(),
                auto_fixable: true,
                fix_hint: Some("Initialize agents.defaults.model".into()),
                source: "primary".into(),
            }],
        };
        let guidance = DocGuidance {
            status: "ok".into(),
            source_strategy: "local-docs-first".into(),
            root_cause_hypotheses: vec![RootCauseHypothesis {
                title: "Agent defaults are missing".into(),
                reason: "The primary profile has no agents.defaults.model binding.".into(),
                score: 0.91,
            }],
            fix_steps: vec![
                "Set agents.defaults.model to a valid provider/model pair.".into(),
                "Re-run the primary check after saving the config.".into(),
            ],
            confidence: 0.91,
            citations: vec![DocCitation {
                url: "https://docs.openclaw.ai/agents".into(),
                section: "defaults".into(),
            }],
            version_awareness: "Guidance matches OpenClaw 2026.3.x.".into(),
            resolver_meta: crate::openclaw_doc_resolver::ResolverMeta {
                cache_hit: false,
                sources_checked: vec!["target-local-docs".into()],
                rules_matched: vec!["agent_workspace_conflict".into()],
                fetched_pages: 1,
                fallback_used: false,
            },
        };

        let enriched = apply_doc_guidance_to_diagnosis(diagnosis, Some(guidance));

        assert_eq!(enriched.summary.root_cause_hypotheses.len(), 1);
        assert_eq!(
            enriched.summary.fix_steps.first().map(String::as_str),
            Some("Set agents.defaults.model to a valid provider/model pair.")
        );
        assert_eq!(
            enriched.summary.recommended_action,
            "Set agents.defaults.model to a valid provider/model pair."
        );
        assert_eq!(enriched.sections[0].key, "agents");
        assert_eq!(enriched.sections[0].citations.len(), 1);
        assert_eq!(
            enriched.sections[0].version_awareness.as_deref(),
            Some("Guidance matches OpenClaw 2026.3.x.")
        );
    }
}
