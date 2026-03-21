use super::*;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupProgressPayload {
    handle_id: String,
    phase: String,
    files_copied: usize,
    bytes_copied: u64,
    current_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupDonePayload {
    handle_id: String,
    info: BackupInfo,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupErrorPayload {
    handle_id: String,
    error: String,
}

#[derive(Debug, Default, Clone)]
struct BackupCopyProgress {
    files_copied: usize,
    bytes_copied: u64,
}

#[tauri::command]
pub async fn remote_backup_before_upgrade(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<BackupInfo, String> {
    timed_async!("remote_backup_before_upgrade", {
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
    })
}

#[tauri::command]
pub async fn backup_before_upgrade_stream(app: AppHandle) -> Result<String, String> {
    timed_async!("backup_before_upgrade_stream", {
        let handle_id = uuid::Uuid::new_v4().to_string();
        let app_handle = app.clone();
        let handle_for_task = handle_id.clone();

        tauri::async_runtime::spawn_blocking(move || {
            let result = run_local_backup_stream(&app_handle, &handle_for_task);
            finalize_backup_stream(&app_handle, &handle_for_task, result);
        });

        Ok(handle_id)
    })
}

#[tauri::command]
pub async fn remote_backup_before_upgrade_stream(
    app: AppHandle,
    host_id: String,
) -> Result<String, String> {
    timed_async!("remote_backup_before_upgrade_stream", {
        let handle_id = uuid::Uuid::new_v4().to_string();
        let app_handle = app.clone();
        let handle_for_task = handle_id.clone();
        let host_for_task = host_id.clone();

        tauri::async_runtime::spawn(async move {
            let pool = app_handle.state::<SshConnectionPool>();
            let result =
                run_remote_backup_stream(&pool, &app_handle, &handle_for_task, &host_for_task)
                    .await;
            finalize_backup_stream(&app_handle, &handle_for_task, result);
        });

        Ok(handle_id)
    })
}

#[tauri::command]
pub async fn remote_list_backups(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<BackupInfo>, String> {
    timed_async!("remote_list_backups", {
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
    })
}

#[tauri::command]
pub async fn remote_restore_from_backup(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    backup_name: String,
) -> Result<String, String> {
    timed_async!("remote_restore_from_backup", {
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
    })
}

#[tauri::command]
pub async fn remote_run_openclaw_upgrade(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<String, String> {
    timed_async!("remote_run_openclaw_upgrade", {
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
        if !version_before.is_empty()
            && !version_after.is_empty()
            && version_before == version_after
        {
            return Err(format!("{combined}\n\nWarning: version unchanged after upgrade ({version_before}). Check PATH or npm prefix."));
        }

        Ok(combined)
    })
}

#[tauri::command]
pub async fn remote_check_openclaw_update(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Value, String> {
    timed_async!("remote_check_openclaw_update", {
        // Get installed version and extract clean semver — don't fail if binary not found
        let installed_version = match pool.exec_login(&host_id, "openclaw --version").await {
            Ok(r) => extract_version_from_text(r.stdout.trim())
                .unwrap_or_else(|| r.stdout.trim().to_string()),
            Err(_) => String::new(),
        };

        let paths = resolve_paths();
        let cache = tokio::task::spawn_blocking(move || {
            resolve_openclaw_latest_release_cached(&paths, false).ok()
        })
        .await
        .unwrap_or(None);
        let latest_version = cache.and_then(|entry| entry.latest_version);
        let upgrade = latest_version
            .as_ref()
            .is_some_and(|latest| compare_semver(&installed_version, Some(latest.as_str())));
        Ok(serde_json::json!({
            "upgradeAvailable": upgrade,
            "latestVersion": latest_version,
            "installedVersion": installed_version,
        }))
    })
}

fn emit_backup_progress(
    app: &AppHandle,
    handle_id: &str,
    phase: &str,
    progress: &BackupCopyProgress,
    current_path: Option<String>,
) {
    let _ = app.emit(
        "backup:progress",
        BackupProgressPayload {
            handle_id: handle_id.to_string(),
            phase: phase.to_string(),
            files_copied: progress.files_copied,
            bytes_copied: progress.bytes_copied,
            current_path,
        },
    );
}

fn finalize_backup_stream(app: &AppHandle, handle_id: &str, result: Result<BackupInfo, String>) {
    match result {
        Ok(info) => {
            let _ = app.emit(
                "backup:done",
                BackupDonePayload {
                    handle_id: handle_id.to_string(),
                    info,
                },
            );
        }
        Err(error) => {
            let _ = app.emit(
                "backup:error",
                BackupErrorPayload {
                    handle_id: handle_id.to_string(),
                    error,
                },
            );
        }
    }
}

fn copy_entry_with_progress(
    src: &Path,
    dst: &Path,
    skip_dirs: &HashSet<&str>,
    progress: &mut BackupCopyProgress,
    app: &AppHandle,
    handle_id: &str,
    phase: &str,
) -> Result<(), String> {
    let metadata =
        fs::metadata(src).map_err(|e| format!("Failed to read {}: {e}", src.display()))?;
    if metadata.is_dir() {
        fs::create_dir_all(dst)
            .map_err(|e| format!("Failed to create dir {}: {e}", dst.display()))?;
        let entries =
            fs::read_dir(src).map_err(|e| format!("Failed to read dir {}: {e}", src.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str == "openclaw.json" || skip_dirs.contains(name_str.as_ref()) {
                continue;
            }
            copy_entry_with_progress(
                &entry.path(),
                &dst.join(&name),
                skip_dirs,
                progress,
                app,
                handle_id,
                phase,
            )?;
        }
    } else if metadata.is_file() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create dir {}: {e}", parent.display()))?;
        }
        fs::copy(src, dst).map_err(|e| format!("Failed to copy {}: {e}", src.display()))?;
        let copied_size = fs::metadata(dst).map(|m| m.len()).unwrap_or(0);
        progress.files_copied += 1;
        progress.bytes_copied = progress.bytes_copied.saturating_add(copied_size);
        emit_backup_progress(
            app,
            handle_id,
            phase,
            progress,
            Some(src.to_string_lossy().to_string()),
        );
    }
    Ok(())
}

fn run_local_backup_stream(app: &AppHandle, handle_id: &str) -> Result<BackupInfo, String> {
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

    let skip_dirs: HashSet<&str> = ["sessions", "archive", ".clawpal"]
        .iter()
        .copied()
        .collect();
    let mut progress = BackupCopyProgress::default();

    emit_backup_progress(app, handle_id, "snapshot", &progress, None);

    if paths.config_path.exists() {
        let dest = backup_dir.join("openclaw.json");
        fs::copy(&paths.config_path, &dest).map_err(|e| format!("Failed to copy config: {e}"))?;
        progress.files_copied += 1;
        progress.bytes_copied = progress
            .bytes_copied
            .saturating_add(fs::metadata(&dest).map(|m| m.len()).unwrap_or(0));
        emit_backup_progress(
            app,
            handle_id,
            "config",
            &progress,
            Some(paths.config_path.to_string_lossy().to_string()),
        );
    }

    let entries = fs::read_dir(&paths.base_dir)
        .map_err(|e| format!("Failed to read base dir {}: {e}", paths.base_dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_string();
        if name_str == "openclaw.json" || skip_dirs.contains(name_str.as_str()) {
            continue;
        }
        let phase = if name_str == "agents" {
            "agents"
        } else if name_str == "memory" {
            "memory"
        } else {
            "snapshot"
        };
        copy_entry_with_progress(
            &entry.path(),
            &backup_dir.join(&name),
            &skip_dirs,
            &mut progress,
            app,
            handle_id,
            phase,
        )?;
    }

    emit_backup_progress(app, handle_id, "done", &progress, None);

    Ok(BackupInfo {
        name: name.clone(),
        path: backup_dir.to_string_lossy().to_string(),
        created_at: format_timestamp_from_unix(now_secs),
        size_bytes: progress.bytes_copied,
    })
}

async fn run_remote_backup_stream(
    pool: &SshConnectionPool,
    app: &AppHandle,
    handle_id: &str,
    host_id: &str,
) -> Result<BackupInfo, String> {
    let now_secs = unix_timestamp_secs();
    let now_dt = chrono::DateTime::<chrono::Utc>::from_timestamp(now_secs as i64, 0);
    let name = now_dt
        .map(|dt| dt.format("%Y-%m-%d_%H%M%S").to_string())
        .unwrap_or_else(|| format!("{now_secs}"));
    let escaped_name = shell_escape(&name);
    let mut progress = BackupCopyProgress::default();

    emit_backup_progress(app, handle_id, "snapshot", &progress, None);
    pool.exec_login(
        host_id,
        &format!(
            "set -e; BDIR=\"$HOME/.clawpal/backups/\"{name}; mkdir -p \"$BDIR\"",
            name = escaped_name
        ),
    )
    .await?;

    let config_result = pool
        .exec_login(
            host_id,
            &format!(
                "set -e; BDIR=\"$HOME/.clawpal/backups/\"{name}; cp \"$HOME/.openclaw/openclaw.json\" \"$BDIR/\" 2>/dev/null || true",
                name = escaped_name
            ),
        )
        .await?;
    if config_result.exit_code != 0 {
        return Err(format!("Remote backup failed: {}", config_result.stderr));
    }
    emit_backup_progress(app, handle_id, "config", &progress, None);

    let agents_result = pool
        .exec_login(
            host_id,
            &format!(
                "set -e; BDIR=\"$HOME/.clawpal/backups/\"{name}; cp -r \"$HOME/.openclaw/agents\" \"$BDIR/\" 2>/dev/null || true",
                name = escaped_name
            ),
        )
        .await?;
    if agents_result.exit_code != 0 {
        return Err(format!("Remote backup failed: {}", agents_result.stderr));
    }
    emit_backup_progress(app, handle_id, "agents", &progress, None);

    let memory_result = pool
        .exec_login(
            host_id,
            &format!(
                "set -e; BDIR=\"$HOME/.clawpal/backups/\"{name}; cp -r \"$HOME/.openclaw/memory\" \"$BDIR/\" 2>/dev/null || true",
                name = escaped_name
            ),
        )
        .await?;
    if memory_result.exit_code != 0 {
        return Err(format!("Remote backup failed: {}", memory_result.stderr));
    }
    emit_backup_progress(app, handle_id, "memory", &progress, None);

    let size_result = pool
        .exec_login(
            host_id,
            &format!(
                "set -e; BDIR=\"$HOME/.clawpal/backups/\"{name}; du -sk \"$BDIR\" 2>/dev/null | awk '{{print $1 * 1024}}' || echo 0",
                name = escaped_name
            ),
        )
        .await?;
    if size_result.exit_code != 0 {
        return Err(format!("Remote backup failed: {}", size_result.stderr));
    }

    let size_bytes = clawpal_core::backup::parse_backup_result(&size_result.stdout).size_bytes;
    progress.bytes_copied = size_bytes;
    emit_backup_progress(app, handle_id, "done", &progress, None);

    Ok(BackupInfo {
        name,
        path: String::new(),
        created_at: format_timestamp_from_unix(now_secs),
        size_bytes,
    })
}

#[tauri::command]
pub fn backup_before_upgrade() -> Result<BackupInfo, String> {
    timed_sync!("backup_before_upgrade", {
        let paths = resolve_paths();
        let backups_dir = paths.clawpal_dir.join("backups");
        fs::create_dir_all(&backups_dir)
            .map_err(|e| format!("Failed to create backups dir: {e}"))?;

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
            fs::copy(&paths.config_path, &dest)
                .map_err(|e| format!("Failed to copy config: {e}"))?;
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
    })
}

#[tauri::command]
pub fn list_backups() -> Result<Vec<BackupInfo>, String> {
    timed_sync!("list_backups", {
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
    })
}

#[tauri::command]
pub fn restore_from_backup(backup_name: String) -> Result<String, String> {
    timed_sync!("restore_from_backup", {
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
    })
}

#[tauri::command]
pub fn delete_backup(backup_name: String) -> Result<bool, String> {
    timed_sync!("delete_backup", {
        let paths = resolve_paths();
        let backup_dir = paths.clawpal_dir.join("backups").join(&backup_name);
        if !backup_dir.exists() {
            return Ok(false);
        }
        fs::remove_dir_all(&backup_dir).map_err(|e| format!("Failed to delete backup: {e}"))?;
        Ok(true)
    })
}

#[tauri::command]
pub async fn remote_delete_backup(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    backup_name: String,
) -> Result<bool, String> {
    timed_async!("remote_delete_backup", {
        let escaped_name = shell_escape(&backup_name);
        let cmd = format!(
            "BDIR=\"$HOME/.clawpal/backups/\"{name}; [ -d \"$BDIR\" ] && rm -rf \"$BDIR\" && echo 'deleted' || echo 'not_found'",
            name = escaped_name
        );

        let result = pool.exec_login(&host_id, &cmd).await?;
        Ok(result.stdout.trim() == "deleted")
    })
}

#[tauri::command]
pub fn check_openclaw_update() -> Result<OpenclawUpdateCheck, String> {
    timed_sync!("check_openclaw_update", {
        let paths = resolve_paths();
        check_openclaw_update_cached(&paths, true)
    })
}

// --- Extracted from mod.rs ---

pub(crate) fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    skip_dirs: &HashSet<&str>,
    total: &mut u64,
) -> Result<(), String> {
    let entries =
        fs::read_dir(src).map_err(|e| format!("Failed to read dir {}: {e}", src.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip the config file (already copied separately) and skip dirs
        if name_str == "openclaw.json" {
            continue;
        }

        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let dest = dst.join(&name);

        if file_type.is_dir() {
            if skip_dirs.contains(name_str.as_ref()) {
                continue;
            }
            fs::create_dir_all(&dest)
                .map_err(|e| format!("Failed to create dir {}: {e}", dest.display()))?;
            copy_dir_recursive(&entry.path(), &dest, skip_dirs, total)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &dest)
                .map_err(|e| format!("Failed to copy {}: {e}", name_str))?;
            *total += fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        }
    }
    Ok(())
}

pub(crate) fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                total += dir_size(&entry.path());
            } else {
                total += fs::metadata(entry.path()).map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

pub(crate) fn restore_dir_recursive(
    src: &Path,
    dst: &Path,
    skip_dirs: &HashSet<&str>,
) -> Result<(), String> {
    let entries = fs::read_dir(src).map_err(|e| format!("Failed to read backup dir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str == "openclaw.json" {
            continue; // Already restored separately
        }

        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let dest = dst.join(&name);

        if file_type.is_dir() {
            if skip_dirs.contains(name_str.as_ref()) {
                continue;
            }
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            restore_dir_recursive(&entry.path(), &dest, skip_dirs)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &dest)
                .map_err(|e| format!("Failed to restore {}: {e}", name_str))?;
        }
    }
    Ok(())
}
