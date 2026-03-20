use std::fs::create_dir_all;
use std::io::Write;
use std::net::TcpStream;
use std::process::Command;
use std::time::Instant;

use serde_json::json;
use tauri::test::mock_app;
use tauri::AppHandle;
use tauri::Manager;
use uuid::Uuid;

use super::agent::detect_method_name;
use super::config::{
    build_gateway_credentials as remote_doctor_gateway_credentials,
    load_gateway_config as remote_doctor_gateway_config,
};
use super::legacy::ensure_rescue_profile_ready;
use super::plan::request_plan;
use super::repair_loops::{
    run_clawpal_server_repair_loop, run_remote_doctor_repair_loop, start_remote_doctor_repair_impl,
};
use super::types::{PlanCommand, PlanKind, PlanResponse, TargetLocation};
use crate::cli_runner::{set_active_clawpal_data_override, set_active_openclaw_home_override};
use crate::node_client::NodeClient;
use crate::ssh::{SshConnectionPool, SshHostConfig};

const E2E_CONTAINER_NAME: &str = "clawpal-e2e-remote-doctor";
const E2E_SSH_PORT: u16 = 2399;
const E2E_ROOT_PASSWORD: &str = "clawpal-remote-doctor-pass";
const E2E_DOCKERFILE: &str = r#"
FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y openssh-server && rm -rf /var/lib/apt/lists/* && mkdir /var/run/sshd
RUN echo "root:ROOTPASS" | chpasswd && \
    sed -i 's/#PermitRootLogin.*/PermitRootLogin yes/' /etc/ssh/sshd_config && \
    sed -i 's/PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config && \
    echo "PasswordAuthentication yes" >> /etc/ssh/sshd_config
RUN mkdir -p /root/.openclaw
RUN cat > /root/.openclaw/openclaw.json <<'EOF'
{
  "gateway": { "port": 18789, "auth": { "token": "gw-test-token" } },
  "auth": {
    "profiles": {
      "openai-default": {
        "provider": "openai",
        "apiKey": "sk-test"
      }
    }
  },
  "models": {
    "providers": {
      "openai": {
        "baseUrl": "http://127.0.0.1:9/v1",
        "models": [{ "id": "gpt-4o-mini", "name": "gpt-4o-mini" }]
      }
    }
  },
  "agents": {
    "defaults": { "model": "openai/gpt-4o-mini" },
    "list": [ { "id": "main", "model": "anthropic/claude-sonnet-4-20250514" } ]
  },
  "channels": {
    "discord": {
      "guilds": {
        "guild-1": {
          "channels": {
            "general": { "model": "openai/gpt-4o-mini" }
          }
        }
      }
    }
  }
}
EOF
RUN cat > /usr/local/bin/openclaw <<'EOF' && chmod +x /usr/local/bin/openclaw
#!/bin/sh
STATE_DIR="${OPENCLAW_STATE_DIR:-${OPENCLAW_HOME:-$HOME/.openclaw}}"
CONFIG_PATH="$STATE_DIR/openclaw.json"
PROFILE="primary"
if [ "$1" = "--profile" ]; then
  PROFILE="$2"
  shift 2
fi
case "$1" in
  --version)
    echo "openclaw 2026.3.2-test"
    ;;
  doctor)
    if grep -q '127.0.0.1:9/v1' "$CONFIG_PATH"; then
      echo '{"ok":false,"score":40,"issues":[{"id":"primary.models.base_url","code":"invalid.base_url","severity":"error","message":"provider baseUrl points to test blackhole","autoFixable":true,"fixHint":"Remove the bad baseUrl override"}]}'
    else
      echo '{"ok":true,"score":100,"issues":[],"checks":[{"id":"test","status":"ok"}]}'
    fi
    ;;
  agents)
    if [ "$2" = "list" ] && [ "$3" = "--json" ]; then
      echo '[{"id":"main"}]'
    else
      echo "unsupported openclaw agents command" >&2
      exit 1
    fi
    ;;
  models)
    if [ "$2" = "list" ] && [ "$3" = "--all" ] && [ "$4" = "--json" ] && [ "$5" = "--no-color" ]; then
      echo '{"models":[{"key":"openai/gpt-4o-mini","provider":"openai","id":"gpt-4o-mini","name":"gpt-4o-mini","baseUrl":"https://api.openai.com/v1"}],"providers":{"openai":{"baseUrl":"https://api.openai.com/v1"}}}'
    else
      echo "unsupported openclaw models command" >&2
      exit 1
    fi
    ;;
  config)
    if [ "$2" = "get" ] && [ "$3" = "gateway.port" ] && [ "$4" = "--json" ]; then
      if [ "$PROFILE" = "rescue" ]; then
        echo '19789'
      else
        echo '18789'
      fi
    else
      echo "unsupported openclaw config command: $*" >&2
      exit 1
    fi
    ;;
  gateway)
    case "$2" in
      status)
        if [ "$PROFILE" = "rescue" ] && [ "${OPENCLAW_RESCUE_GATEWAY_ACTIVE:-1}" != "1" ]; then
          echo '{"running":false,"healthy":false,"gateway":{"running":false},"health":{"ok":false}}'
        else
          echo '{"running":true,"healthy":true,"gateway":{"running":true},"health":{"ok":true}}'
        fi
        ;;
      restart|start|stop)
        echo '{"ok":true}'
        ;;
      *)
        echo "unsupported openclaw gateway command: $*" >&2
        exit 1
        ;;
    esac
    ;;
  *)
    echo "unsupported openclaw command: $*" >&2
    exit 1
    ;;
esac
EOF
EXPOSE 22
CMD ["/usr/sbin/sshd", "-D"]
"#;

fn should_run_docker_e2e() -> bool {
    std::env::var("CLAWPAL_RUN_REMOTE_DOCTOR_E2E")
        .ok()
        .as_deref()
        == Some("1")
}

fn live_gateway_url() -> Option<String> {
    std::env::var("CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn live_gateway_token() -> Option<String> {
    std::env::var("CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn live_gateway_instance_id() -> String {
    std::env::var("CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_INSTANCE_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local".to_string())
}

fn live_gateway_target_location() -> TargetLocation {
    match std::env::var("CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_TARGET_LOCATION")
        .ok()
        .as_deref()
    {
        Some("remote_openclaw") => TargetLocation::RemoteOpenclaw,
        _ => TargetLocation::LocalOpenclaw,
    }
}

fn live_gateway_protocol() -> String {
    std::env::var("CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_PROTOCOL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "clawpal_server".to_string())
}

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn cleanup_e2e_container() {
    let _ = Command::new("docker")
        .args(["rm", "-f", E2E_CONTAINER_NAME])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = Command::new("docker")
        .args(["rmi", "-f", &format!("{E2E_CONTAINER_NAME}:latest")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

fn build_e2e_image() -> Result<(), String> {
    let dockerfile = E2E_DOCKERFILE.replace("ROOTPASS", E2E_ROOT_PASSWORD);
    let output = Command::new("docker")
        .args([
            "build",
            "-t",
            &format!("{E2E_CONTAINER_NAME}:latest"),
            "-f",
            "-",
            ".",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .current_dir(std::env::temp_dir())
        .spawn()
        .and_then(|mut child| {
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(dockerfile.as_bytes())?;
            }
            child.wait_with_output()
        })
        .map_err(|error| format!("docker build failed: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

fn start_e2e_container() -> Result<(), String> {
    start_e2e_container_with_env(&[])
}

fn start_e2e_container_with_env(env: &[(&str, &str)]) -> Result<(), String> {
    let mut args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--name".to_string(),
        E2E_CONTAINER_NAME.to_string(),
    ];
    for (key, value) in env {
        args.push("-e".to_string());
        args.push(format!("{key}={value}"));
    }
    args.extend([
        "-p".to_string(),
        format!("{E2E_SSH_PORT}:22"),
        format!("{E2E_CONTAINER_NAME}:latest"),
    ]);
    let output = Command::new("docker")
        .args(&args)
        .output()
        .map_err(|error| format!("docker run failed: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

fn wait_for_ssh(timeout_secs: u64) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if TcpStream::connect(format!("127.0.0.1:{E2E_SSH_PORT}")).is_ok() {
            std::thread::sleep(std::time::Duration::from_millis(500));
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    Err("timeout waiting for ssh".into())
}

fn e2e_host_config() -> SshHostConfig {
    SshHostConfig {
        id: "e2e-remote-doctor".into(),
        label: "E2E Remote Doctor".into(),
        host: "127.0.0.1".into(),
        port: E2E_SSH_PORT,
        username: "root".into(),
        auth_method: "password".into(),
        key_path: None,
        password: Some(E2E_ROOT_PASSWORD.into()),
        passphrase: None,
    }
}

#[tokio::test]
async fn remote_doctor_docker_e2e_loop_completes() {
    if !should_run_docker_e2e() {
        eprintln!("skip: set CLAWPAL_RUN_REMOTE_DOCTOR_E2E=1 to enable");
        return;
    }
    if !docker_available() {
        eprintln!("skip: docker not available");
        return;
    }

    cleanup_e2e_container();
    build_e2e_image().expect("docker build");
    start_e2e_container().expect("docker run");
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            cleanup_e2e_container();
        }
    }
    let _cleanup = Cleanup;
    wait_for_ssh(30).expect("ssh should become available");

    let temp_root =
        std::env::temp_dir().join(format!("clawpal-remote-doctor-e2e-{}", Uuid::new_v4()));
    let clawpal_dir = temp_root.join(".clawpal");
    create_dir_all(&clawpal_dir).expect("create clawpal dir");
    set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
        .expect("set clawpal data");
    set_active_openclaw_home_override(None).expect("clear openclaw home override");

    let pool = SshConnectionPool::new();
    let cfg = e2e_host_config();
    pool.connect(&cfg).await.expect("ssh connect");

    let session_id = Uuid::new_v4().to_string();
    let marker = "/tmp/clawpal-remote-doctor-fixed";
    let result = run_remote_doctor_repair_loop(
        Option::<&AppHandle<tauri::test::MockRuntime>>::None,
        &pool,
        &session_id,
        &format!("ssh:{}", cfg.id),
        TargetLocation::RemoteOpenclaw,
        |kind, round, previous_results: Vec<crate::remote_doctor::types::CommandResult>| async move {
            match (kind, round) {
                (PlanKind::Detect, 1) => Ok(PlanResponse {
                    plan_id: "detect-1".into(),
                    plan_kind: PlanKind::Detect,
                    summary: "Initial detect".into(),
                    commands: vec![PlanCommand {
                        argv: vec!["openclaw".into(), "--version".into()],
                        timeout_sec: Some(10),
                        purpose: Some("collect version".into()),
                        continue_on_failure: Some(false),
                    }],
                    healthy: false,
                    done: false,
                    success: false,
                }),
                (PlanKind::Repair, 1) => {
                    assert_eq!(previous_results.len(), 1);
                    Ok(PlanResponse {
                        plan_id: "repair-1".into(),
                        plan_kind: PlanKind::Repair,
                        summary: "Write marker".into(),
                        commands: vec![PlanCommand {
                            argv: vec![
                                "sh".into(),
                                "-lc".into(),
                                format!("printf 'fixed' > {marker}"),
                            ],
                            timeout_sec: Some(10),
                            purpose: Some("mark repaired".into()),
                            continue_on_failure: Some(false),
                        }],
                        healthy: false,
                        done: false,
                        success: false,
                    })
                }
                (PlanKind::Detect, 2) => {
                    assert_eq!(previous_results.len(), 1);
                    assert_eq!(previous_results[0].stdout.trim(), "");
                    Ok(PlanResponse {
                        plan_id: "detect-2".into(),
                        plan_kind: PlanKind::Detect,
                        summary: "Marker exists".into(),
                        commands: Vec::new(),
                        healthy: true,
                        done: true,
                        success: true,
                    })
                }
                _ => Err(format!(
                    "unexpected planner request: {:?} round {}",
                    kind, round
                )),
            }
        },
    )
    .await
    .expect("remote doctor loop should complete");

    assert_eq!(result.status, "completed");
    assert!(result.latest_diagnosis_healthy);
    assert_eq!(result.round, 2);

    let marker_result = pool
        .exec(&cfg.id, &format!("test -f {marker}"))
        .await
        .expect("marker check");
    assert_eq!(marker_result.exit_code, 0);

    let log_path = clawpal_dir
        .join("doctor")
        .join("remote")
        .join(format!("{session_id}.jsonl"));
    let log_text = std::fs::read_to_string(&log_path).expect("read remote doctor log");
    assert!(log_text.contains("\"planKind\":\"detect\""));
    assert!(log_text.contains("\"planKind\":\"repair\""));
    let _ = std::fs::remove_dir_all(temp_root);
    set_active_clawpal_data_override(None).expect("clear clawpal data");
}

#[tokio::test]
async fn remote_doctor_docker_e2e_rescue_activation_fails_when_gateway_stays_inactive() {
    if !should_run_docker_e2e() {
        eprintln!("skip: set CLAWPAL_RUN_REMOTE_DOCTOR_E2E=1 to enable");
        return;
    }
    if !docker_available() {
        eprintln!("skip: docker not available");
        return;
    }

    cleanup_e2e_container();
    build_e2e_image().expect("docker build");
    start_e2e_container_with_env(&[("OPENCLAW_RESCUE_GATEWAY_ACTIVE", "0")]).expect("docker run");
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            cleanup_e2e_container();
        }
    }
    let _cleanup = Cleanup;
    wait_for_ssh(30).expect("ssh should become available");

    let app = mock_app();
    let app_handle = app.handle().clone();
    app_handle.manage(SshConnectionPool::new());
    let pool = app_handle.state::<SshConnectionPool>();
    let cfg = e2e_host_config();
    pool.connect(&cfg).await.expect("ssh connect");

    let error = ensure_rescue_profile_ready(
        &app_handle,
        TargetLocation::RemoteOpenclaw,
        &format!("ssh:{}", cfg.id),
    )
    .await
    .expect_err("rescue activation should fail when gateway remains inactive");

    assert!(error.message.contains("did not become active"));
    assert!(error.message.contains("configured_inactive"));
    assert!(error
        .diagnostics
        .iter()
        .any(|result| result.argv.join(" ") == "manage_rescue_bot status rescue"));
}

#[tokio::test]
async fn remote_doctor_live_gateway_uses_configured_url_and_token() {
    let Some(url) = live_gateway_url() else {
        eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_URL to enable");
        return;
    };
    let Some(token) = live_gateway_token() else {
        eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_TOKEN to enable");
        return;
    };

    let app = mock_app();
    let app_handle = app.handle().clone();
    app_handle.manage(SshConnectionPool::new());
    let temp_root =
        std::env::temp_dir().join(format!("clawpal-remote-doctor-live-{}", Uuid::new_v4()));
    let clawpal_dir = temp_root.join(".clawpal");
    create_dir_all(&clawpal_dir).expect("create clawpal dir");
    set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
        .expect("set clawpal data");

    std::fs::write(
        clawpal_dir.join("app-preferences.json"),
        serde_json::to_string(&json!({
            "remoteDoctorGatewayUrl": url,
            "remoteDoctorGatewayAuthToken": token,
        }))
        .expect("serialize prefs"),
    )
    .expect("write app preferences");

    let gateway = remote_doctor_gateway_config().expect("gateway config");
    assert_eq!(gateway.url, url);
    assert_eq!(gateway.auth_token_override.as_deref(), Some(token.as_str()));

    let creds = remote_doctor_gateway_credentials(gateway.auth_token_override.as_deref())
        .expect("gateway credentials");
    assert!(creds.is_some());

    let client = NodeClient::new();
    client
        .connect(&gateway.url, app.handle().clone(), creds)
        .await
        .expect("connect live remote doctor gateway");
    assert!(client.is_connected().await);
    match live_gateway_protocol().as_str() {
        "clawpal_server" => {
            let response = client
                .send_request(
                    "remote_repair_plan.request",
                    json!({
                        "requestId": format!("live-e2e-{}", Uuid::new_v4()),
                        "targetId": live_gateway_instance_id(),
                        "context": {
                            "configExcerpt": {
                                "models": {
                                    "providers": {
                                        "openai-codex": {
                                            "baseUrl": "http://127.0.0.1:9/v1"
                                        }
                                    }
                                }
                            }
                        }
                    }),
                )
                .await
                .expect("request clawpal-server remote repair plan");
            let plan_id = response
                .get("planId")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            assert!(!plan_id.trim().is_empty());
            let steps = response
                .get("steps")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            assert!(!steps.is_empty());
        }
        _ => {
            let detect_plan = request_plan(
                &client,
                &detect_method_name(),
                PlanKind::Detect,
                &format!("live-e2e-{}", Uuid::new_v4()),
                1,
                live_gateway_target_location(),
                &live_gateway_instance_id(),
                &[],
            )
            .await
            .expect("request live detection plan");
            assert!(!detect_plan.plan_id.trim().is_empty());
        }
    }
    client.disconnect().await.expect("disconnect");

    set_active_clawpal_data_override(None).expect("clear clawpal data");
    let _ = std::fs::remove_dir_all(temp_root);
}

#[tokio::test]
async fn remote_doctor_live_gateway_full_repair_loop_completes() {
    let Some(url) = live_gateway_url() else {
        eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_URL to enable");
        return;
    };
    let Some(token) = live_gateway_token() else {
        eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_TOKEN to enable");
        return;
    };
    if !docker_available() {
        eprintln!("skip: docker not available");
        return;
    }

    cleanup_e2e_container();
    build_e2e_image().expect("docker build");
    start_e2e_container().expect("docker run");
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            cleanup_e2e_container();
        }
    }
    let _cleanup = Cleanup;
    wait_for_ssh(30).expect("ssh should become available");

    let app = mock_app();
    let app_handle = app.handle().clone();
    app_handle.manage(SshConnectionPool::new());
    let temp_root = std::env::temp_dir().join(format!(
        "clawpal-remote-doctor-live-loop-{}",
        Uuid::new_v4()
    ));
    let clawpal_dir = temp_root.join(".clawpal");
    create_dir_all(&clawpal_dir).expect("create clawpal dir");
    set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
        .expect("set clawpal data");
    set_active_openclaw_home_override(None).expect("clear openclaw home override");

    std::fs::write(
        clawpal_dir.join("app-preferences.json"),
        serde_json::to_string(&json!({
            "remoteDoctorGatewayUrl": url,
            "remoteDoctorGatewayAuthToken": token,
        }))
        .expect("serialize prefs"),
    )
    .expect("write app preferences");

    let cfg = e2e_host_config();
    let pool = app_handle.state::<SshConnectionPool>();
    pool.connect(&cfg).await.expect("ssh connect");

    let gateway = remote_doctor_gateway_config().expect("gateway config");
    let creds = remote_doctor_gateway_credentials(gateway.auth_token_override.as_deref())
        .expect("gateway credentials");
    let client = NodeClient::new();
    client
        .connect(&gateway.url, app_handle.clone(), creds)
        .await
        .expect("connect live remote doctor gateway");

    let session_id = Uuid::new_v4().to_string();
    let result = run_clawpal_server_repair_loop(
        &app_handle,
        &client,
        &session_id,
        &format!("ssh:{}", cfg.id),
        TargetLocation::RemoteOpenclaw,
    )
    .await
    .expect("full live remote doctor repair loop should complete");

    assert_eq!(result.status, "completed");
    assert!(result.latest_diagnosis_healthy);

    client.disconnect().await.expect("disconnect");
    set_active_clawpal_data_override(None).expect("clear clawpal data");
    let _ = std::fs::remove_dir_all(temp_root);
}

#[tokio::test]
async fn remote_doctor_live_start_command_remote_target_completes_without_bridge_pairing() {
    let Some(url) = live_gateway_url() else {
        eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_URL to enable");
        return;
    };
    let Some(token) = live_gateway_token() else {
        eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_TOKEN to enable");
        return;
    };
    if !docker_available() {
        eprintln!("skip: docker not available");
        return;
    }

    cleanup_e2e_container();
    build_e2e_image().expect("docker build");
    start_e2e_container().expect("docker run");
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            cleanup_e2e_container();
        }
    }
    let _cleanup = Cleanup;
    wait_for_ssh(30).expect("ssh should become available");

    let app = mock_app();
    let app_handle = app.handle().clone();
    app_handle.manage(SshConnectionPool::new());
    let temp_root = std::env::temp_dir().join(format!(
        "clawpal-remote-doctor-live-start-{}",
        Uuid::new_v4()
    ));
    let clawpal_dir = temp_root.join(".clawpal");
    create_dir_all(&clawpal_dir).expect("create clawpal dir");
    set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
        .expect("set clawpal data");
    set_active_openclaw_home_override(None).expect("clear openclaw home override");

    std::fs::write(
        clawpal_dir.join("app-preferences.json"),
        serde_json::to_string(&json!({
            "remoteDoctorGatewayUrl": url,
            "remoteDoctorGatewayAuthToken": token,
        }))
        .expect("serialize prefs"),
    )
    .expect("write app preferences");

    let cfg = crate::commands::ssh::upsert_ssh_host(e2e_host_config()).expect("save ssh host");
    let pool = app_handle.state::<SshConnectionPool>();

    let result = start_remote_doctor_repair_impl(
        app_handle.clone(),
        &pool,
        format!("ssh:{}", cfg.id),
        "remote_openclaw".to_string(),
    )
    .await
    .expect("start command should complete remote repair");

    assert_eq!(result.status, "completed");
    assert!(result.latest_diagnosis_healthy);

    let log_path = clawpal_dir
        .join("doctor")
        .join("remote")
        .join(format!("{}.jsonl", result.session_id));
    let log_text = std::fs::read_to_string(&log_path).expect("read remote doctor session log");
    assert!(
        !log_text.contains("\"event\":\"bridge_connect_failed\""),
        "clawpal_server path should not attempt bridge pairing: {log_text}"
    );

    set_active_clawpal_data_override(None).expect("clear clawpal data");
    let _ = std::fs::remove_dir_all(temp_root);
}

#[tokio::test]
async fn remote_doctor_live_gateway_repairs_unreadable_remote_config() {
    let Some(url) = live_gateway_url() else {
        eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_URL to enable");
        return;
    };
    let Some(token) = live_gateway_token() else {
        eprintln!("skip: set CLAWPAL_REMOTE_DOCTOR_LIVE_E2E_TOKEN to enable");
        return;
    };
    if !docker_available() {
        eprintln!("skip: docker not available");
        return;
    }

    cleanup_e2e_container();
    build_e2e_image().expect("docker build");
    start_e2e_container().expect("docker run");
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            cleanup_e2e_container();
        }
    }
    let _cleanup = Cleanup;
    wait_for_ssh(30).expect("ssh should become available");

    let app = mock_app();
    let app_handle = app.handle().clone();
    app_handle.manage(SshConnectionPool::new());
    let temp_root = std::env::temp_dir().join(format!(
        "clawpal-remote-doctor-live-raw-config-{}",
        Uuid::new_v4()
    ));
    let clawpal_dir = temp_root.join(".clawpal");
    create_dir_all(&clawpal_dir).expect("create clawpal dir");
    set_active_clawpal_data_override(Some(clawpal_dir.to_string_lossy().to_string()))
        .expect("set clawpal data");
    set_active_openclaw_home_override(None).expect("clear openclaw home override");

    std::fs::write(
        clawpal_dir.join("app-preferences.json"),
        serde_json::to_string(&json!({
            "remoteDoctorGatewayUrl": url,
            "remoteDoctorGatewayAuthToken": token,
        }))
        .expect("serialize prefs"),
    )
    .expect("write app preferences");

    let cfg = crate::commands::ssh::upsert_ssh_host(e2e_host_config()).expect("save ssh host");
    let pool = app_handle.state::<SshConnectionPool>();
    pool.connect(&cfg).await.expect("ssh connect");
    pool.exec_login(
        &cfg.id,
        "cat > ~/.openclaw/openclaw.json <<'EOF'\n{\n  ddd\n}\nEOF",
    )
    .await
    .expect("corrupt remote config");

    let result = start_remote_doctor_repair_impl(
        app_handle.clone(),
        &pool,
        cfg.id.clone(),
        "remote_openclaw".to_string(),
    )
    .await
    .expect("start command should repair unreadable config");

    assert_eq!(result.status, "completed");
    assert!(result.latest_diagnosis_healthy);

    let repaired = pool
        .exec_login(&cfg.id, "python3 - <<'PY'\nimport json, pathlib\njson.load(open(pathlib.Path.home()/'.openclaw'/'openclaw.json'))\nprint('ok')\nPY")
        .await
        .expect("read repaired config");
    assert_eq!(
        repaired.exit_code, 0,
        "repaired config should be valid JSON: {}",
        repaired.stderr
    );
    assert_eq!(repaired.stdout.trim(), "ok");

    set_active_clawpal_data_override(None).expect("clear clawpal data");
    let _ = std::fs::remove_dir_all(temp_root);
}
