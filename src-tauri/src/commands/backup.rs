use super::*;

#[tauri::command]
pub async fn remote_backup_before_upgrade(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<BackupInfo, String> {
    let now_secs = unix_timestamp_secs();
    let now_dt = chrono::DateTime::<chrono::Utc>::from_timestamp(now_secs as i64, 0);
    let name = now_dt
        .map(|dt| dt.format("%Y-%m-%d_%H%M%S").to_string())
        .unwrap_or_else(|| format!("{now_secs}"));

    let escaped_name = shell_escape(&name);
    let cmd = format!(
        concat!(
            "set -e; ",
            "BDIR=\"$HOME/.clawpal/backups/\"{name}; ",
            "mkdir -p \"$BDIR\"; ",
            "cp \"$HOME/.openclaw/openclaw.json\" \"$BDIR/\" 2>/dev/null || true; ",
            "cp -r \"$HOME/.openclaw/agents\" \"$BDIR/\" 2>/dev/null || true; ",
            "cp -r \"$HOME/.openclaw/memory\" \"$BDIR/\" 2>/dev/null || true; ",
            "du -sk \"$BDIR\" 2>/dev/null | awk '{{print $1 * 1024}}' || echo 0"
        ),
        name = escaped_name
    );

    let result = pool.exec_login(&host_id, &cmd).await?;
    if result.exit_code != 0 {
        return Err(format!(
            "Remote backup failed (exit {}): {}",
            result.exit_code, result.stderr
        ));
    }

    let size_bytes = clawpal_core::backup::parse_backup_result(&result.stdout).size_bytes;

    Ok(BackupInfo {
        name,
        path: String::new(),
        created_at: format_timestamp_from_unix(now_secs),
        size_bytes,
    })
}

#[tauri::command]
pub async fn remote_list_backups(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<BackupInfo>, String> {
    // Migrate remote data from legacy path ~/.openclaw/.clawpal → ~/.clawpal
    let _ = pool
        .exec_login(
            &host_id,
            concat!(
                "if [ -d \"$HOME/.openclaw/.clawpal\" ]; then ",
                "mkdir -p \"$HOME/.clawpal\"; ",
                "cp -a \"$HOME/.openclaw/.clawpal/.\" \"$HOME/.clawpal/\" 2>/dev/null; ",
                "rm -rf \"$HOME/.openclaw/.clawpal\"; ",
                "fi"
            ),
        )
        .await;

    // List backup directory names
    let list_result = pool
        .exec_login(
            &host_id,
            "ls -1d \"$HOME/.clawpal/backups\"/*/  2>/dev/null || true",
        )
        .await?;

    let dirs: Vec<String> = list_result
        .stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().trim_end_matches('/').to_string())
        .collect();

    if dirs.is_empty() {
        return Ok(Vec::new());
    }

    // Build a single command to get sizes for all backup dirs (du -sk is POSIX portable)
    let du_parts: Vec<String> = dirs
        .iter()
        .map(|d| format!("du -sk '{}' 2>/dev/null || echo '0\t{}'", d, d))
        .collect();
    let du_cmd = du_parts.join("; ");
    let du_result = pool.exec_login(&host_id, &du_cmd).await?;

    let size_entries = clawpal_core::backup::parse_backup_list(&du_result.stdout);
    let size_map: std::collections::HashMap<String, u64> = size_entries
        .into_iter()
        .map(|e| (e.path, e.size_bytes))
        .collect();

    let mut backups: Vec<BackupInfo> = dirs
        .iter()
        .map(|d| {
            let name = d.rsplit('/').next().unwrap_or(d).to_string();
            let size_bytes = size_map.get(d.trim_end_matches('/')).copied().unwrap_or(0);
            BackupInfo {
                name: name.clone(),
                path: d.clone(),
                created_at: name.clone(), // Name is the timestamp
                size_bytes,
            }
        })
        .collect();

    backups.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(backups)
}

#[tauri::command]
pub async fn remote_restore_from_backup(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    backup_name: String,
) -> Result<String, String> {
    let escaped_name = shell_escape(&backup_name);
    let cmd = format!(
        concat!(
            "set -e; ",
            "BDIR=\"$HOME/.clawpal/backups/\"{name}; ",
            "[ -d \"$BDIR\" ] || {{ echo 'Backup not found'; exit 1; }}; ",
            "cp \"$BDIR/openclaw.json\" \"$HOME/.openclaw/openclaw.json\" 2>/dev/null || true; ",
            "[ -d \"$BDIR/agents\" ] && cp -r \"$BDIR/agents\" \"$HOME/.openclaw/\" 2>/dev/null || true; ",
            "[ -d \"$BDIR/memory\" ] && cp -r \"$BDIR/memory\" \"$HOME/.openclaw/\" 2>/dev/null || true; ",
            "echo 'Restored from backup '{name}"
        ),
        name = escaped_name
    );

    let result = pool.exec_login(&host_id, &cmd).await?;
    if result.exit_code != 0 {
        return Err(format!("Remote restore failed: {}", result.stderr));
    }

    Ok(format!("Restored from backup '{}'", backup_name))
}

#[tauri::command]
pub async fn remote_run_openclaw_upgrade(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<String, String> {
    // Use the official install script with --no-prompt for non-interactive SSH.
    // The script handles npm prefix/permissions, bin links, and PATH fixups
    // that plain `npm install -g` misses (e.g. stale /usr/bin/openclaw symlinks).
    let version_before = pool
        .exec_login(&host_id, "openclaw --version 2>/dev/null || true")
        .await
        .map(|r| r.stdout.trim().to_string())
        .unwrap_or_default();

    let install_cmd = "curl -fsSL --proto '=https' --tlsv1.2 https://openclaw.ai/install.sh | bash -s -- --no-prompt --no-onboard 2>&1";
    let result = pool.exec_login(&host_id, install_cmd).await?;
    let combined = if result.stderr.is_empty() {
        result.stdout.clone()
    } else {
        format!("{}\n{}", result.stdout, result.stderr)
    };

    if result.exit_code != 0 {
        return Err(combined);
    }

    // Restart gateway after successful upgrade (best-effort)
    let _ = pool
        .exec_login(&host_id, "openclaw gateway restart 2>/dev/null || true")
        .await;

    // Verify version actually changed
    let version_after = pool
        .exec_login(&host_id, "openclaw --version 2>/dev/null || true")
        .await
        .map(|r| r.stdout.trim().to_string())
        .unwrap_or_default();
    let _upgrade_info = clawpal_core::backup::parse_upgrade_result(&combined);
    if !version_before.is_empty() && !version_after.is_empty() && version_before == version_after {
        return Err(format!("{combined}\n\nWarning: version unchanged after upgrade ({version_before}). Check PATH or npm prefix."));
    }

    Ok(combined)
}

#[tauri::command]
pub async fn remote_check_openclaw_update(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Value, String> {
    // Get installed version and extract clean semver — don't fail if binary not found
    let installed_version = match pool.exec_login(&host_id, "openclaw --version").await {
        Ok(r) => extract_version_from_text(r.stdout.trim())
            .unwrap_or_else(|| r.stdout.trim().to_string()),
        Err(_) => String::new(),
    };

    // Try `openclaw update status --json` first (may not exist on older versions)
    let update_result = pool
        .exec_login(
            &host_id,
            "openclaw update status --json --no-color 2>/dev/null",
        )
        .await;
    if let Ok(r) = update_result {
        if r.exit_code == 0 && !r.stdout.trim().is_empty() {
            if let Some((latest, _channel, _details, upgrade)) =
                parse_openclaw_update_json(&r.stdout, &installed_version)
            {
                return Ok(serde_json::json!({
                    "upgradeAvailable": upgrade,
                    "latestVersion": latest,
                    "installedVersion": installed_version,
                }));
            }
        }
    }

    // Fallback: query npm registry directly from Tauri (no remote CLI dependency)
    // Must use spawn_blocking because reqwest::blocking panics in async context
    let latest_version = tokio::task::spawn_blocking(|| query_openclaw_latest_npm().ok().flatten())
        .await
        .unwrap_or(None);
    let upgrade = latest_version
        .as_ref()
        .is_some_and(|latest| compare_semver(&installed_version, Some(latest.as_str())));
    Ok(serde_json::json!({
        "upgradeAvailable": upgrade,
        "latestVersion": latest_version,
        "installedVersion": installed_version,
    }))
}

#[tauri::command]
pub fn backup_before_upgrade() -> Result<BackupInfo, String> {
    let paths = resolve_paths();
    let backups_dir = paths.clawpal_dir.join("backups");
    fs::create_dir_all(&backups_dir).map_err(|e| format!("Failed to create backups dir: {e}"))?;

    let now_secs = unix_timestamp_secs();
    let now_dt = chrono::DateTime::<chrono::Utc>::from_timestamp(now_secs as i64, 0);
    let name = now_dt
        .map(|dt| dt.format("%Y-%m-%d_%H%M%S").to_string())
        .unwrap_or_else(|| format!("{now_secs}"));
    let backup_dir = backups_dir.join(&name);
    fs::create_dir_all(&backup_dir).map_err(|e| format!("Failed to create backup dir: {e}"))?;

    let mut total_bytes = 0u64;

    // Copy config file
    if paths.config_path.exists() {
        let dest = backup_dir.join("openclaw.json");
        fs::copy(&paths.config_path, &dest).map_err(|e| format!("Failed to copy config: {e}"))?;
        total_bytes += fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    }

    // Copy directories, excluding sessions and archive
    let skip_dirs: HashSet<&str> = ["sessions", "archive", ".clawpal"]
        .iter()
        .copied()
        .collect();
    copy_dir_recursive(&paths.base_dir, &backup_dir, &skip_dirs, &mut total_bytes)?;

    Ok(BackupInfo {
        name: name.clone(),
        path: backup_dir.to_string_lossy().to_string(),
        created_at: format_timestamp_from_unix(now_secs),
        size_bytes: total_bytes,
    })
}

#[tauri::command]
pub fn list_backups() -> Result<Vec<BackupInfo>, String> {
    let paths = resolve_paths();
    let backups_dir = paths.clawpal_dir.join("backups");
    if !backups_dir.exists() {
        return Ok(Vec::new());
    }
    let mut backups = Vec::new();
    let entries = fs::read_dir(&backups_dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let size = dir_size(&path);
        let created_at = fs::metadata(&path)
            .and_then(|m| m.created())
            .map(|t| {
                let secs = t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                format_timestamp_from_unix(secs)
            })
            .unwrap_or_else(|_| name.clone());
        backups.push(BackupInfo {
            name,
            path: path.to_string_lossy().to_string(),
            created_at,
            size_bytes: size,
        });
    }
    backups.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(backups)
}

#[tauri::command]
pub fn restore_from_backup(backup_name: String) -> Result<String, String> {
    let paths = resolve_paths();
    let backup_dir = paths.clawpal_dir.join("backups").join(&backup_name);
    if !backup_dir.exists() {
        return Err(format!("Backup '{}' not found", backup_name));
    }

    // Restore config file
    let backup_config = backup_dir.join("openclaw.json");
    if backup_config.exists() {
        fs::copy(&backup_config, &paths.config_path)
            .map_err(|e| format!("Failed to restore config: {e}"))?;
    }

    // Restore other directories (agents except sessions/archive, memory, etc.)
    let skip_dirs: HashSet<&str> = ["sessions", "archive", ".clawpal"]
        .iter()
        .copied()
        .collect();
    restore_dir_recursive(&backup_dir, &paths.base_dir, &skip_dirs)?;

    Ok(format!("Restored from backup '{}'", backup_name))
}

#[tauri::command]
pub fn delete_backup(backup_name: String) -> Result<bool, String> {
    let paths = resolve_paths();
    let backup_dir = paths.clawpal_dir.join("backups").join(&backup_name);
    if !backup_dir.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(&backup_dir).map_err(|e| format!("Failed to delete backup: {e}"))?;
    Ok(true)
}

#[tauri::command]
pub async fn remote_delete_backup(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    backup_name: String,
) -> Result<bool, String> {
    let escaped_name = shell_escape(&backup_name);
    let cmd = format!(
        "BDIR=\"$HOME/.clawpal/backups/\"{name}; [ -d \"$BDIR\" ] && rm -rf \"$BDIR\" && echo 'deleted' || echo 'not_found'",
        name = escaped_name
    );

    let result = pool.exec_login(&host_id, &cmd).await?;
    Ok(result.stdout.trim() == "deleted")
}

#[tauri::command]
pub fn check_openclaw_update() -> Result<OpenclawUpdateCheck, String> {
    let paths = resolve_paths();
    check_openclaw_update_cached(&paths, true)
}
