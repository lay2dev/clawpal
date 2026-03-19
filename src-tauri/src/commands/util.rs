use super::*;

use std::process::Command;

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    timed_sync!("open_url", {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err("URL is required".into());
        }
        // Allow http(s) URLs and local paths within user home directory
        if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
            // For local paths, ensure they don't execute apps
            let path = std::path::Path::new(trimmed);
            if path
                .extension()
                .map_or(false, |ext| ext == "app" || ext == "exe")
            {
                return Err("Cannot open application files".into());
            }
        }
        #[cfg(target_os = "macos")]
        {
            Command::new("open")
                .arg(&url)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        #[cfg(target_os = "linux")]
        {
            Command::new("xdg-open")
                .arg(&url)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        #[cfg(target_os = "windows")]
        {
            Command::new("cmd")
                .args(["/c", "start", &url])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })
}
