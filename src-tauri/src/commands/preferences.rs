use serde::{Deserialize, Serialize};

use crate::config_io::{read_json, write_json};
use crate::models::{resolve_paths, OpenClawPaths};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zeroclaw_model: Option<String>,
}

fn app_preferences_path(paths: &OpenClawPaths) -> std::path::PathBuf {
    paths.clawpal_dir.join("app-preferences.json")
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

pub fn load_app_preferences_from_paths(paths: &OpenClawPaths) -> AppPreferences {
    let path = app_preferences_path(paths);
    let mut prefs = read_json::<AppPreferences>(&path).unwrap_or_default();
    prefs.zeroclaw_model = normalize_optional_string(prefs.zeroclaw_model);
    prefs
}

fn save_app_preferences_from_paths(
    paths: &OpenClawPaths,
    prefs: &AppPreferences,
) -> Result<(), String> {
    let path = app_preferences_path(paths);
    write_json(&path, prefs)
}

pub fn load_zeroclaw_model_preference() -> Option<String> {
    let paths = resolve_paths();
    load_app_preferences_from_paths(&paths).zeroclaw_model
}

#[tauri::command]
pub fn get_app_preferences() -> Result<AppPreferences, String> {
    let paths = resolve_paths();
    Ok(load_app_preferences_from_paths(&paths))
}

#[tauri::command]
pub fn set_zeroclaw_model_preference(model: Option<String>) -> Result<AppPreferences, String> {
    let paths = resolve_paths();
    let mut prefs = load_app_preferences_from_paths(&paths);
    prefs.zeroclaw_model = normalize_optional_string(model);
    save_app_preferences_from_paths(&paths, &prefs)?;
    Ok(prefs)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ZeroclawUsageStatsResponse {
    pub total_calls: u64,
    pub usage_calls: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub last_updated_ms: u64,
}

#[tauri::command]
pub fn get_zeroclaw_usage_stats() -> Result<ZeroclawUsageStatsResponse, String> {
    let stats = crate::runtime::zeroclaw::process::get_zeroclaw_usage_stats();
    Ok(ZeroclawUsageStatsResponse {
        total_calls: stats.total_calls,
        usage_calls: stats.usage_calls,
        prompt_tokens: stats.prompt_tokens,
        completion_tokens: stats.completion_tokens,
        total_tokens: stats.total_tokens,
        last_updated_ms: stats.last_updated_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_optional_string_trims_and_drops_empty_values() {
        assert_eq!(
            normalize_optional_string(Some("  openai/gpt-4.1  ".into())),
            Some("openai/gpt-4.1".into())
        );
        assert_eq!(normalize_optional_string(Some("   ".into())), None);
        assert_eq!(normalize_optional_string(None), None);
    }
}
