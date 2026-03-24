use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use uuid::Uuid;

fn temp_home_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "clawpal-remote-doctor-bootstrap-script-{label}-{}",
        Uuid::new_v4()
    ))
}

fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .join("scripts")
        .join("remote-doctor-bootstrap.sh")
}

fn seed_openclaw_config(home_dir: &Path, config_text: &str) -> PathBuf {
    let openclaw_dir = home_dir.join(".openclaw");
    fs::create_dir_all(&openclaw_dir).expect("create openclaw dir");
    let config_path = openclaw_dir.join("openclaw.json");
    fs::write(&config_path, config_text).expect("write config");
    config_path
}

fn run_bootstrap_script(home_dir: &Path) -> std::process::Output {
    Command::new("bash")
        .arg(script_path())
        .env("HOME", home_dir)
        .output()
        .expect("run bootstrap script")
}

fn backup_files(config_path: &Path) -> Vec<PathBuf> {
    let parent = config_path.parent().expect("config parent");
    let prefix = format!(
        "{}.bak-",
        config_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("config file name")
    );
    fs::read_dir(parent)
        .expect("read config dir")
        .filter_map(|entry| entry.ok().map(|item| item.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with(&prefix))
                .unwrap_or(false)
        })
        .collect()
}

fn load_config(config_path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(config_path).expect("read config"))
        .expect("parse config")
}

fn assert_bootstrap_workspace(home_dir: &Path) {
    let workspace = home_dir
        .join(".openclaw")
        .join("workspaces")
        .join("clawpal-remote-doctor");
    for file_name in [
        "IDENTITY.md",
        "AGENTS.md",
        "BOOTSTRAP.md",
        "USER.md",
        "HEARTBEAT.md",
    ] {
        let path = workspace.join(file_name);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert!(
            !text.trim().is_empty(),
            "{file_name} should not be empty in {}",
            workspace.display()
        );
    }
}

#[test]
fn bootstrap_script_adds_remote_doctor_agent_and_workspace() {
    let home_dir = temp_home_dir("basic");
    fs::create_dir_all(&home_dir).expect("create temp home");
    let config_path = seed_openclaw_config(
        &home_dir,
        r#"{
  "agents": {
    "list": [
      { "id": "main", "workspace": "~/.openclaw/workspaces/main" }
    ]
  }
}
"#,
    );

    let output = run_bootstrap_script(&home_dir);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\n\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(backup_files(&config_path).len(), 1, "expected one config backup");

    let config = load_config(&config_path);
    let agents = config
        .pointer("/agents/list")
        .and_then(Value::as_array)
        .expect("agents.list array");
    let remote_doctor = agents
        .iter()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some("clawpal-remote-doctor"))
        .expect("remote doctor agent entry");
    assert_eq!(
        remote_doctor.get("workspace").and_then(Value::as_str),
        Some("~/.openclaw/workspaces/clawpal-remote-doctor")
    );

    assert_bootstrap_workspace(&home_dir);

    let _ = fs::remove_dir_all(&home_dir);
}

#[test]
fn bootstrap_script_accepts_comment_and_trailing_comma_config() {
    let home_dir = temp_home_dir("json5");
    fs::create_dir_all(&home_dir).expect("create temp home");
    let config_path = seed_openclaw_config(
        &home_dir,
        r#"{
  // existing agent
  "agents": {
    "list": [
      { "id": "main", "workspace": "~/.openclaw/workspaces/main", },
    ],
  },
}
"#,
    );

    let output = run_bootstrap_script(&home_dir);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\n\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let config = load_config(&config_path);
    let agents = config
        .pointer("/agents/list")
        .and_then(Value::as_array)
        .expect("agents.list array");
    let matches = agents
        .iter()
        .filter(|entry| entry.get("id").and_then(Value::as_str) == Some("clawpal-remote-doctor"))
        .count();
    assert_eq!(matches, 1, "remote doctor agent should be added exactly once");

    assert_bootstrap_workspace(&home_dir);

    let _ = fs::remove_dir_all(&home_dir);
}
