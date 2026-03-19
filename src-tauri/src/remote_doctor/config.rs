use std::fs::create_dir_all;
use std::path::PathBuf;

use ed25519_dalek::pkcs8::EncodePrivateKey;
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, Runtime};

use super::session::append_session_log;
use super::types::{
    diagnosis_issue_summaries, ConfigExcerptContext, StoredRemoteDoctorIdentity, TargetLocation,
};
use crate::commands::preferences::load_app_preferences_from_paths;
use crate::commands::{
    diagnose_primary_via_rescue, read_raw_config, remote_diagnose_primary_via_rescue,
    remote_read_raw_config, remote_restart_gateway, remote_write_raw_config, restart_gateway,
    RescuePrimaryDiagnosisResult, RescuePrimarySummary,
};
use crate::commands::version::{format_timestamp_from_unix, unix_timestamp_secs};
use crate::models::resolve_paths;
use crate::node_client::GatewayCredentials;
use crate::ssh::SshConnectionPool;

const DEFAULT_GATEWAY_HOST: &str = "127.0.0.1";
const DEFAULT_GATEWAY_PORT: u16 = 18789;

#[derive(Debug, Clone)]
pub(crate) struct RemoteDoctorGatewayConfig {
    pub(crate) url: String,
    pub(crate) auth_token_override: Option<String>,
}

pub(crate) fn load_gateway_config() -> Result<RemoteDoctorGatewayConfig, String> {
    let paths = resolve_paths();
    let app_preferences = load_app_preferences_from_paths(&paths);
    if let Some(url) = app_preferences.remote_doctor_gateway_url {
        return Ok(RemoteDoctorGatewayConfig {
            url,
            auth_token_override: app_preferences.remote_doctor_gateway_auth_token,
        });
    }
    let configured_port = std::fs::read_to_string(&paths.config_path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|config| {
            config
                .get("gateway")
                .and_then(|gateway| gateway.get("port"))
                .and_then(|value| value.as_u64())
        })
        .map(|value| value as u16)
        .unwrap_or(DEFAULT_GATEWAY_PORT);
    Ok(RemoteDoctorGatewayConfig {
        url: format!("ws://{DEFAULT_GATEWAY_HOST}:{configured_port}"),
        auth_token_override: app_preferences.remote_doctor_gateway_auth_token,
    })
}

pub(crate) fn build_gateway_credentials(
    auth_token_override: Option<&str>,
) -> Result<Option<GatewayCredentials>, String> {
    let Some(token) = auth_token_override.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let identity = load_or_create_remote_doctor_identity()?;
    Ok(Some(GatewayCredentials {
        token: token.to_string(),
        device_id: identity.device_id,
        private_key_pem: identity.private_key_pem,
    }))
}

pub(crate) fn remote_doctor_identity_path() -> PathBuf {
    resolve_paths()
        .clawpal_dir
        .join("remote-doctor")
        .join("device-identity.json")
}

pub(crate) fn load_or_create_remote_doctor_identity() -> Result<StoredRemoteDoctorIdentity, String>
{
    let path = remote_doctor_identity_path();
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(identity) = serde_json::from_str::<StoredRemoteDoctorIdentity>(&text) {
            if identity.version == 1
                && !identity.device_id.trim().is_empty()
                && !identity.private_key_pem.trim().is_empty()
            {
                return Ok(identity);
            }
        }
    }

    let parent = path
        .parent()
        .ok_or("Failed to resolve remote doctor identity directory")?;
    create_dir_all(parent)
        .map_err(|e| format!("Failed to create remote doctor identity dir: {e}"))?;

    let mut secret = [0u8; 32];
    getrandom::getrandom(&mut secret)
        .map_err(|e| format!("Failed to generate remote doctor device secret: {e}"))?;
    let signing_key = SigningKey::from_bytes(&secret);
    let raw_public = signing_key.verifying_key().to_bytes();
    let device_id = Sha256::digest(raw_public)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let private_key_pem = signing_key
        .to_pkcs8_pem(Default::default())
        .map_err(|e| format!("Failed to encode remote doctor private key: {e}"))?
        .to_string();
    let created_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("Failed to get system time: {e}"))?
        .as_millis() as u64;
    let identity = StoredRemoteDoctorIdentity {
        version: 1,
        created_at_ms,
        device_id,
        private_key_pem,
    };
    let text = serde_json::to_string_pretty(&identity)
        .map_err(|e| format!("Failed to serialize remote doctor identity: {e}"))?;
    std::fs::write(&path, format!("{text}\n"))
        .map_err(|e| format!("Failed to persist remote doctor identity: {e}"))?;
    Ok(identity)
}

pub(crate) fn build_config_excerpt_context(raw: &str) -> ConfigExcerptContext {
    match serde_json::from_str::<Value>(raw) {
        Ok(config_excerpt) => ConfigExcerptContext {
            config_excerpt,
            config_excerpt_raw: None,
            config_parse_error: None,
        },
        Err(error) => ConfigExcerptContext {
            config_excerpt: Value::Null,
            config_excerpt_raw: Some(raw.to_string()),
            config_parse_error: Some(format!("Failed to parse target config: {error}")),
        },
    }
}

pub(crate) fn config_excerpt_log_summary(context: &ConfigExcerptContext) -> Value {
    json!({
        "configExcerptPresent": !context.config_excerpt.is_null(),
        "configExcerptBytes": serde_json::to_string(&context.config_excerpt).ok().map(|text| text.len()).unwrap_or(0),
        "configExcerptRawPresent": context.config_excerpt_raw.as_ref().map(|text| !text.trim().is_empty()).unwrap_or(false),
        "configExcerptRawBytes": context.config_excerpt_raw.as_ref().map(|text| text.len()).unwrap_or(0),
        "configParseError": context.config_parse_error,
    })
}

pub(crate) fn empty_config_excerpt_context() -> ConfigExcerptContext {
    ConfigExcerptContext {
        config_excerpt: Value::Null,
        config_excerpt_raw: None,
        config_parse_error: None,
    }
}

pub(crate) fn empty_diagnosis() -> RescuePrimaryDiagnosisResult {
    RescuePrimaryDiagnosisResult {
        status: "healthy".into(),
        checked_at: format_timestamp_from_unix(unix_timestamp_secs()),
        target_profile: "primary".into(),
        rescue_profile: "rescue".into(),
        rescue_configured: false,
        rescue_port: None,
        summary: RescuePrimarySummary {
            status: "healthy".into(),
            headline: "Healthy".into(),
            recommended_action: "No action needed".into(),
            fixable_issue_count: 0,
            selected_fix_issue_ids: Vec::new(),
            root_cause_hypotheses: Vec::new(),
            fix_steps: Vec::new(),
            confidence: None,
            citations: Vec::new(),
            version_awareness: None,
        },
        sections: Vec::new(),
        checks: Vec::new(),
        issues: Vec::new(),
    }
}

pub(crate) fn diagnosis_has_only_non_auto_fixable_issues(
    diagnosis: &RescuePrimaryDiagnosisResult,
) -> bool {
    !diagnosis.issues.is_empty() && diagnosis.issues.iter().all(|issue| !issue.auto_fixable)
}

pub(crate) fn diagnosis_is_healthy(diagnosis: &RescuePrimaryDiagnosisResult) -> bool {
    diagnosis.status == "healthy"
        && diagnosis.summary.status == "healthy"
        && diagnosis.issues.is_empty()
}

pub(crate) fn diagnosis_context(diagnosis: &RescuePrimaryDiagnosisResult) -> Value {
    json!({
        "status": diagnosis.status,
        "summary": {
            "status": diagnosis.summary.status,
            "headline": diagnosis.summary.headline,
            "recommendedAction": diagnosis.summary.recommended_action,
            "fixableIssueCount": diagnosis.summary.fixable_issue_count,
            "selectedFixIssueIds": diagnosis.summary.selected_fix_issue_ids,
        },
        "issues": diagnosis.issues,
        "sections": diagnosis.sections,
    })
}

pub(crate) fn diagnosis_missing_rescue_profile(diagnosis: &RescuePrimaryDiagnosisResult) -> bool {
    diagnosis
        .issues
        .iter()
        .any(|issue| issue.code == "rescue.profile.missing")
}

pub(crate) fn diagnosis_unhealthy_rescue_gateway(diagnosis: &RescuePrimaryDiagnosisResult) -> bool {
    diagnosis
        .issues
        .iter()
        .any(|issue| issue.code == "rescue.gateway.unhealthy")
}

pub(crate) fn append_diagnosis_log(
    session_id: &str,
    stage: &str,
    round: usize,
    diagnosis: &RescuePrimaryDiagnosisResult,
) {
    append_session_log(
        session_id,
        json!({
            "event": "diagnosis_result",
            "stage": stage,
            "round": round,
            "status": diagnosis.status,
            "summaryStatus": diagnosis.summary.status,
            "headline": diagnosis.summary.headline,
            "recommendedAction": diagnosis.summary.recommended_action,
            "issueCount": diagnosis.issues.len(),
            "issues": diagnosis_issue_summaries(diagnosis),
        }),
    );
}

pub(crate) fn remote_target_host_id_candidates(instance_id: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let trimmed = instance_id.trim();
    if !trimmed.is_empty() {
        candidates.push(trimmed.to_string());
    }
    if let Some(stripped) = trimmed.strip_prefix("ssh:").map(str::trim) {
        if !stripped.is_empty() && !candidates.iter().any(|value| value == stripped) {
            candidates.push(stripped.to_string());
        }
    }
    candidates
}

pub(crate) fn primary_remote_target_host_id(instance_id: &str) -> Result<String, String> {
    remote_target_host_id_candidates(instance_id)
        .into_iter()
        .next()
        .ok_or_else(|| "Remote Doctor repair requires an ssh instance id".to_string())
}

pub(crate) async fn run_rescue_diagnosis<R: Runtime>(
    app: &AppHandle<R>,
    target_location: TargetLocation,
    instance_id: &str,
) -> Result<RescuePrimaryDiagnosisResult, String> {
    match target_location {
        TargetLocation::LocalOpenclaw => diagnose_primary_via_rescue(None, None).await,
        TargetLocation::RemoteOpenclaw => {
            let host_id = primary_remote_target_host_id(instance_id)?;
            remote_diagnose_primary_via_rescue(
                app.state::<SshConnectionPool>(),
                host_id,
                None,
                None,
            )
            .await
        }
    }
}

pub(crate) async fn read_target_config<R: Runtime>(
    app: &AppHandle<R>,
    target_location: TargetLocation,
    instance_id: &str,
) -> Result<Value, String> {
    let raw = match target_location {
        TargetLocation::LocalOpenclaw => read_raw_config()?,
        TargetLocation::RemoteOpenclaw => {
            let host_id = primary_remote_target_host_id(instance_id)?;
            remote_read_raw_config(app.state::<SshConnectionPool>(), host_id).await?
        }
    };
    serde_json::from_str::<Value>(&raw)
        .map_err(|error| format!("Failed to parse target config: {error}"))
}

pub(crate) async fn read_target_config_raw<R: Runtime>(
    app: &AppHandle<R>,
    target_location: TargetLocation,
    instance_id: &str,
) -> Result<String, String> {
    match target_location {
        TargetLocation::LocalOpenclaw => read_raw_config(),
        TargetLocation::RemoteOpenclaw => {
            let host_id = primary_remote_target_host_id(instance_id)?;
            remote_read_raw_config(app.state::<SshConnectionPool>(), host_id).await
        }
    }
}

pub(crate) async fn write_target_config<R: Runtime>(
    app: &AppHandle<R>,
    target_location: TargetLocation,
    instance_id: &str,
    config: &Value,
) -> Result<(), String> {
    let text = serde_json::to_string_pretty(config).map_err(|error| error.to_string())?;
    let validated = clawpal_core::config::validate_config_json(&text)
        .map_err(|error| format!("Invalid config after remote doctor patch: {error}"))?;
    let validated_text =
        serde_json::to_string_pretty(&validated).map_err(|error| error.to_string())?;
    match target_location {
        TargetLocation::LocalOpenclaw => {
            let paths = resolve_paths();
            crate::config_io::write_text(&paths.config_path, &validated_text)?;
        }
        TargetLocation::RemoteOpenclaw => {
            let host_id = primary_remote_target_host_id(instance_id)?;
            remote_write_raw_config(app.state::<SshConnectionPool>(), host_id, validated_text)
                .await?;
        }
    }
    Ok(())
}

pub(crate) async fn write_target_config_raw<R: Runtime>(
    app: &AppHandle<R>,
    target_location: TargetLocation,
    instance_id: &str,
    text: &str,
) -> Result<(), String> {
    let validated = clawpal_core::config::validate_config_json(text)
        .map_err(|error| format!("Invalid raw config payload: {error}"))?;
    let validated_text =
        serde_json::to_string_pretty(&validated).map_err(|error| error.to_string())?;
    match target_location {
        TargetLocation::LocalOpenclaw => {
            let paths = resolve_paths();
            crate::config_io::write_text(&paths.config_path, &validated_text)?;
        }
        TargetLocation::RemoteOpenclaw => {
            let host_id = primary_remote_target_host_id(instance_id)?;
            remote_write_raw_config(app.state::<SshConnectionPool>(), host_id, validated_text)
                .await?;
        }
    }
    Ok(())
}

pub(crate) async fn restart_target_gateway<R: Runtime>(
    app: &AppHandle<R>,
    target_location: TargetLocation,
    instance_id: &str,
) -> Result<(), String> {
    match target_location {
        TargetLocation::LocalOpenclaw => restart_gateway().await.map(|_| ()),
        TargetLocation::RemoteOpenclaw => {
            let host_id = primary_remote_target_host_id(instance_id)?;
            remote_restart_gateway(app.state::<SshConnectionPool>(), host_id)
                .await
                .map(|_| ())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;
    use crate::cli_runner::{set_active_clawpal_data_override, set_active_openclaw_home_override};

    fn override_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn load_gateway_config_prefers_app_preferences() {
        let _guard = override_lock().lock().expect("lock override state");
        let temp_root = std::env::temp_dir().join(format!(
            "clawpal-remote-doctor-config-pref-test-{}",
            uuid::Uuid::new_v4()
        ));
        let clawpal_dir = temp_root.join(".clawpal");
        let openclaw_dir = temp_root.join(".openclaw");
        std::fs::create_dir_all(&clawpal_dir).expect("create clawpal dir");
        std::fs::create_dir_all(&openclaw_dir).expect("create openclaw dir");
        set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
            .expect("set clawpal override");

        std::fs::write(
            clawpal_dir.join("app-preferences.json"),
            serde_json::to_string(&json!({
                "remoteDoctorGatewayUrl": "ws://example.test:9999",
                "remoteDoctorGatewayAuthToken": "abc",
            }))
            .expect("serialize prefs"),
        )
        .expect("write prefs");

        let config = load_gateway_config().expect("load gateway config");
        assert_eq!(config.url, "ws://example.test:9999");
        assert_eq!(config.auth_token_override.as_deref(), Some("abc"));

        set_active_clawpal_data_override(None).expect("clear clawpal override");
        let _ = std::fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn build_gateway_credentials_returns_none_for_empty_override() {
        let result = build_gateway_credentials(Some("   ")).expect("build credentials");
        assert!(result.is_none());
    }

    #[test]
    fn load_or_create_remote_doctor_identity_persists_usable_identity() {
        let _guard = override_lock().lock().expect("lock override state");
        let temp_root = std::env::temp_dir().join(format!(
            "clawpal-remote-doctor-identity-test-{}",
            uuid::Uuid::new_v4()
        ));
        let clawpal_dir = temp_root.join(".clawpal");
        std::fs::create_dir_all(&clawpal_dir).expect("create clawpal dir");
        set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
            .expect("set clawpal override");

        let identity = load_or_create_remote_doctor_identity().expect("create identity");
        assert_eq!(identity.version, 1);
        assert!(!identity.device_id.is_empty());
        assert!(!identity.private_key_pem.is_empty());
        assert!(remote_doctor_identity_path().exists());

        set_active_clawpal_data_override(None).expect("clear clawpal override");
        let _ = std::fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn build_config_excerpt_context_records_parse_errors() {
        let context = build_config_excerpt_context("{\n  ddd\n}");
        assert!(context.config_excerpt.is_null());
        assert!(context.config_excerpt_raw.is_some());
        assert!(context
            .config_parse_error
            .as_deref()
            .unwrap_or_default()
            .contains("Failed to parse target config"));
    }

    #[test]
    fn unreadable_config_context_summary_marks_excerpt_missing() {
        let context = build_config_excerpt_context("{\n  ddd\n}");
        let summary = config_excerpt_log_summary(&context);
        assert_eq!(summary["configExcerptPresent"], json!(false));
        assert_eq!(summary["configExcerptRawPresent"], json!(true));
        assert!(summary["configParseError"]
            .as_str()
            .unwrap_or_default()
            .contains("Failed to parse target config"));
    }

    #[test]
    fn empty_diagnosis_checked_at_is_not_hardcoded() {
        let diagnosis = empty_diagnosis();
        assert_ne!(diagnosis.checked_at, "2026-03-18T00:00:00Z");
    }

    #[test]
    fn diagnosis_missing_rescue_profile_is_detected() {
        let diagnosis = empty_diagnosis_with_issues(vec![json!({
            "id": "rescue.profile.missing",
            "code": "rescue.profile.missing",
            "severity": "error",
            "message": "Rescue profile missing",
            "autoFixable": false,
            "fixHint": "Activate Rescue Bot first",
            "source": "rescue"
        })]);
        assert!(diagnosis_missing_rescue_profile(&diagnosis));
    }

    #[test]
    fn diagnosis_unhealthy_rescue_gateway_is_detected() {
        let diagnosis = empty_diagnosis_with_issues(vec![json!({
            "id": "rescue.gateway.unhealthy",
            "code": "rescue.gateway.unhealthy",
            "severity": "warn",
            "message": "Rescue gateway unhealthy",
            "autoFixable": false,
            "fixHint": "Inspect rescue gateway",
            "source": "rescue"
        })]);
        assert!(diagnosis_unhealthy_rescue_gateway(&diagnosis));
    }

    #[test]
    fn non_auto_fixable_warning_only_diagnosis_is_terminal() {
        let diagnosis = empty_diagnosis_with_issues(vec![json!({
            "id": "rescue.gateway.unhealthy",
            "code": "rescue.gateway.unhealthy",
            "severity": "warn",
            "message": "Rescue gateway unhealthy",
            "autoFixable": false,
            "fixHint": "Inspect rescue gateway",
            "source": "rescue"
        })]);
        assert!(diagnosis_has_only_non_auto_fixable_issues(&diagnosis));
    }

    #[test]
    fn remote_target_host_id_candidates_include_exact_and_stripped_ids() {
        assert_eq!(
            remote_target_host_id_candidates("ssh:15-235-214-81"),
            vec!["ssh:15-235-214-81".to_string(), "15-235-214-81".to_string()]
        );
        assert_eq!(
            remote_target_host_id_candidates("e2e-remote-doctor"),
            vec!["e2e-remote-doctor".to_string()]
        );
    }

    #[test]
    fn primary_remote_target_host_id_prefers_exact_instance_id() {
        assert_eq!(
            primary_remote_target_host_id("ssh:15-235-214-81").unwrap(),
            "ssh:15-235-214-81"
        );
    }

    #[tokio::test]
    async fn read_target_config_raw_returns_current_file_contents() {
        let _guard = override_lock().lock().expect("lock override state");
        let app = tauri::test::mock_app();
        let temp_root = std::env::temp_dir().join(format!(
            "clawpal-remote-doctor-read-config-test-{}",
            uuid::Uuid::new_v4()
        ));
        let openclaw_home = temp_root.join("home");
        let openclaw_dir = openclaw_home.join(".openclaw");
        std::fs::create_dir_all(&openclaw_dir).expect("create openclaw dir");
        set_active_openclaw_home_override(Some(openclaw_home.to_string_lossy().to_string()))
            .expect("set openclaw override");
        let raw = "{\n  \"ok\": true\n}";
        std::fs::write(openclaw_dir.join("openclaw.json"), raw).expect("write config");

        let result =
            read_target_config_raw(&app.handle().clone(), TargetLocation::LocalOpenclaw, "")
                .await
                .expect("read raw config");

        set_active_openclaw_home_override(None).expect("clear openclaw override");
        let _ = std::fs::remove_dir_all(&temp_root);

        assert!(result.contains("\"ok\": true"));
    }

    fn empty_diagnosis_with_issues(issues: Vec<Value>) -> RescuePrimaryDiagnosisResult {
        serde_json::from_value(json!({
            "status": if issues.is_empty() { "healthy" } else { "broken" },
            "checkedAt": "2026-03-18T00:00:00Z",
            "targetProfile": "primary",
            "rescueProfile": "rescue",
            "rescueConfigured": true,
            "rescuePort": 18789,
            "summary": {
                "status": if issues.is_empty() { "healthy" } else { "broken" },
                "headline": if issues.is_empty() { "Healthy" } else { "Broken" },
                "recommendedAction": if issues.is_empty() { "No action needed" } else { "Repair issues" },
                "fixableIssueCount": issues.len(),
                "selectedFixIssueIds": issues.iter().filter_map(|issue| issue.get("id").and_then(Value::as_str)).collect::<Vec<_>>(),
                "rootCauseHypotheses": [],
                "fixSteps": [],
                "confidence": 0.8,
                "citations": [],
                "versionAwareness": null
            },
            "sections": [],
            "checks": [],
            "issues": issues
        }))
        .expect("sample diagnosis")
    }
}
