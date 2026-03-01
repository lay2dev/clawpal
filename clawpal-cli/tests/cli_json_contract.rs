use std::process::Command;

use serde_json::Value;

fn run_cli(args: &[&str], data_dir: &std::path::Path) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_clawpal-cli"))
        .args(args)
        .env("CLAWPAL_DATA_DIR", data_dir)
        .output()
        .expect("failed to run clawpal-cli");
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    (code, json)
}

fn temp_data_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("clawpal-cli-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp data dir");
    dir
}

#[test]
fn instance_list_outputs_json_array() {
    let dir = temp_data_dir();
    let (code, json) = run_cli(&["instance", "list"], &dir);
    assert_eq!(code, 0);
    assert!(json.is_array(), "expected array, got: {json}");
}

#[test]
fn ssh_list_outputs_json_array() {
    let dir = temp_data_dir();
    let (code, json) = run_cli(&["ssh", "list"], &dir);
    assert_eq!(code, 0);
    assert!(json.is_array(), "expected array, got: {json}");
}

#[test]
fn ssh_disconnect_outputs_json_object() {
    let dir = temp_data_dir();
    let (code, json) = run_cli(&["ssh", "disconnect", "demo-host"], &dir);
    assert_eq!(code, 0);
    assert_eq!(
        json.get("hostId").and_then(|v| v.as_str()),
        Some("demo-host")
    );
    assert_eq!(
        json.get("disconnected").and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn ssh_connect_missing_instance_returns_error_json() {
    let dir = temp_data_dir();
    let (code, json) = run_cli(&["ssh", "connect", "missing-host"], &dir);
    assert_eq!(code, 1);
    let err = json
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    assert!(
        err.contains("not found"),
        "expected not found error, got: {err}"
    );
}
