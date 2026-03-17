use super::*;

const MAX_LOG_TAIL_LINES: usize = 400;

fn clamp_log_lines(lines: Option<usize>) -> usize {
    let requested = lines.unwrap_or(200);
    requested.clamp(1, MAX_LOG_TAIL_LINES)
}

#[tauri::command]
pub fn read_app_log(lines: Option<usize>) -> Result<String, String> {
    timed_sync!("read_app_log", {
    crate::logging::read_log_tail("app.log", clamp_log_lines(lines))
    })
}

#[tauri::command]
pub fn read_error_log(lines: Option<usize>) -> Result<String, String> {
    timed_sync!("read_error_log", {
    crate::logging::read_log_tail("error.log", clamp_log_lines(lines))
    })
}

#[tauri::command]
pub fn read_helper_log(lines: Option<usize>) -> Result<String, String> {
    timed_sync!("read_helper_log", {
    crate::logging::read_log_tail("helper.log", clamp_log_lines(lines))
    })
}

#[tauri::command]
pub fn log_app_event(message: String) -> Result<bool, String> {
    timed_sync!("log_app_event", {
    let trimmed = message.trim();
    if !trimmed.is_empty() {
        crate::logging::log_info(trimmed);
    }
    Ok(true)
    })
}

#[tauri::command]
pub fn read_gateway_log(lines: Option<usize>) -> Result<String, String> {
    timed_sync!("read_gateway_log", {
    let paths = crate::models::resolve_paths();
    let path = paths.openclaw_dir.join("logs/gateway.log");
    if !path.exists() {
        return Ok(String::new());
    }
    crate::logging::read_path_tail(&path, clamp_log_lines(lines))
    })
}

#[tauri::command]
pub fn read_gateway_error_log(lines: Option<usize>) -> Result<String, String> {
    timed_sync!("read_gateway_error_log", {
    let paths = crate::models::resolve_paths();
    let path = paths.openclaw_dir.join("logs/gateway.err.log");
    if !path.exists() {
        return Ok(String::new());
    }
    crate::logging::read_path_tail(&path, clamp_log_lines(lines))
    })
}
