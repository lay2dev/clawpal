use super::*;

#[tauri::command]
pub async fn remote_list_cron_jobs(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Value, String> {
    let raw = pool.sftp_read(&host_id, "~/.openclaw/cron/jobs.json").await;
    match raw {
        Ok(text) => Ok(parse_cron_jobs(&text)),
        Err(_) => Ok(Value::Array(vec![])),
    }
}

#[tauri::command]
pub async fn remote_get_cron_runs(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    job_id: String,
    limit: Option<usize>,
) -> Result<Vec<Value>, String> {
    let path = format!("~/.openclaw/cron/runs/{}.jsonl", job_id);
    let raw = pool.sftp_read(&host_id, &path).await;
    match raw {
        Ok(text) => {
            let mut runs = clawpal_core::cron::parse_cron_runs(&text)?;
            let limit = limit.unwrap_or(10);
            runs.truncate(limit);
            Ok(runs)
        }
        Err(_) => Ok(vec![]),
    }
}

#[tauri::command]
pub async fn remote_trigger_cron_job(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    job_id: String,
) -> Result<String, String> {
    let result = pool
        .exec_login(
            &host_id,
            &format!("openclaw cron run {}", shell_escape(&job_id)),
        )
        .await?;
    if result.exit_code == 0 {
        Ok(result.stdout)
    } else {
        Err(format!("{}\n{}", result.stdout, result.stderr))
    }
}

#[tauri::command]
pub async fn remote_delete_cron_job(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
    job_id: String,
) -> Result<String, String> {
    let result = pool
        .exec_login(
            &host_id,
            &format!("openclaw cron remove {}", shell_escape(&job_id)),
        )
        .await?;
    if result.exit_code == 0 {
        Ok(result.stdout)
    } else {
        Err(format!("{}\n{}", result.stdout, result.stderr))
    }
}

#[tauri::command]
pub fn list_cron_jobs() -> Result<Value, String> {
    let paths = resolve_paths();
    let jobs_path = paths.base_dir.join("cron").join("jobs.json");
    if !jobs_path.exists() {
        return Ok(Value::Array(vec![]));
    }
    let text = std::fs::read_to_string(&jobs_path).map_err(|e| e.to_string())?;
    Ok(parse_cron_jobs(&text))
}

#[tauri::command]
pub fn get_cron_runs(job_id: String, limit: Option<usize>) -> Result<Vec<Value>, String> {
    let paths = resolve_paths();
    let runs_path = paths
        .base_dir
        .join("cron")
        .join("runs")
        .join(format!("{}.jsonl", job_id));
    if !runs_path.exists() {
        return Ok(vec![]);
    }
    let text = std::fs::read_to_string(&runs_path).map_err(|e| e.to_string())?;
    let mut runs = clawpal_core::cron::parse_cron_runs(&text)?;
    let limit = limit.unwrap_or(10);
    runs.truncate(limit);
    Ok(runs)
}

#[tauri::command]
pub async fn trigger_cron_job(job_id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = std::process::Command::new(clawpal_core::openclaw::resolve_openclaw_bin());
        cmd.args(["cron", "run", &job_id]);
        if let Some(path) = crate::cli_runner::get_active_openclaw_home_override() {
            cmd.env("OPENCLAW_HOME", path);
        }
        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run openclaw: {e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if output.status.success() {
            Ok(stdout)
        } else {
            // Extract meaningful error lines, skip Doctor warning banners
            let error_msg =
                clawpal_core::doctor::strip_doctor_banner(&format!("{stdout}\n{stderr}"));
            Err(error_msg)
        }
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

#[tauri::command]
pub fn delete_cron_job(job_id: String) -> Result<String, String> {
    let mut cmd = std::process::Command::new(clawpal_core::openclaw::resolve_openclaw_bin());
    cmd.args(["cron", "remove", &job_id]);
    if let Some(path) = crate::cli_runner::get_active_openclaw_home_override() {
        cmd.env("OPENCLAW_HOME", path);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run openclaw: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        Err(format!("{stdout}\n{stderr}"))
    }
}
