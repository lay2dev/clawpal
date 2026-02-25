use super::{run_command, RunnerFailure, RunnerOutput};
use crate::install::types::InstallStep;
use serde_json::Value;
use std::collections::HashMap;

fn resolve_target(artifacts: &HashMap<String, Value>) -> Result<String, RunnerFailure> {
    if let Some(target) = artifacts.get("ssh_target").and_then(Value::as_str) {
        let trimmed = target.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Ok(target) = std::env::var("CLAWPAL_INSTALL_REMOTE_HOST") {
        let trimmed = target.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    Err(RunnerFailure {
        error_code: "validation_failed".to_string(),
        summary: "remote ssh target missing".to_string(),
        details: "Set session artifact 'ssh_target' or env CLAWPAL_INSTALL_REMOTE_HOST".to_string(),
        commands: vec![],
    })
}

pub fn run_step(
    step: &InstallStep,
    artifacts: &HashMap<String, Value>,
) -> Result<RunnerOutput, RunnerFailure> {
    let target = resolve_target(artifacts)?;
    match step {
        InstallStep::Precheck => {
            let check = run_command("ssh", &[target.as_str(), "echo clawpal-ssh-ok"])?;
            Ok(RunnerOutput {
                summary: "remote ssh precheck completed".to_string(),
                details: check.stdout,
                commands: vec![check.command_line],
                artifacts: HashMap::new(),
            })
        }
        InstallStep::Install => {
            let script = "command -v openclaw >/dev/null 2>&1 || (curl -fsSL --proto '=https' --tlsv1.2 https://openclaw.ai/install.sh | bash -s -- --no-prompt --no-onboard)";
            let install = run_command("ssh", &[target.as_str(), script])?;
            Ok(RunnerOutput {
                summary: "remote ssh install completed".to_string(),
                details: if install.stderr.is_empty() {
                    install.stdout
                } else {
                    format!("{}\n{}", install.stdout, install.stderr)
                },
                commands: vec![install.command_line],
                artifacts: HashMap::new(),
            })
        }
        InstallStep::Init => {
            let init = run_command(
                "ssh",
                &[
                    target.as_str(),
                    "mkdir -p ~/.openclaw && [ -f ~/.openclaw/openclaw.json ] || printf '{}' > ~/.openclaw/openclaw.json",
                ],
            )?;
            Ok(RunnerOutput {
                summary: "remote ssh init completed".to_string(),
                details: if init.stdout.is_empty() {
                    "Initialized ~/.openclaw on remote host".to_string()
                } else {
                    init.stdout
                },
                commands: vec![init.command_line],
                artifacts: HashMap::new(),
            })
        }
        InstallStep::Verify => {
            let verify = run_command(
                "ssh",
                &[
                    target.as_str(),
                    "openclaw --version && openclaw config get agents --json >/dev/null",
                ],
            )?;
            Ok(RunnerOutput {
                summary: "remote ssh verify completed".to_string(),
                details: verify.stdout,
                commands: vec![verify.command_line],
                artifacts: HashMap::new(),
            })
        }
    }
}
