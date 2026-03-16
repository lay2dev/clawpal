use std::process::Command;

#[tauri::command]
pub async fn run_openclaw_upgrade() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "curl -fsSL https://openclaw.ai/install.sh | bash"])
        .output()
        .map_err(|e| format!("Failed to run upgrade: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = if stderr.is_empty() {
        stdout
    } else {
        format!("{stdout}\n{stderr}")
    };
    if output.status.success() {
        super::clear_openclaw_version_cache();
        Ok(combined)
    } else {
        Err(combined)
    }
}
