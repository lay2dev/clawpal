use super::{run_command, RunnerFailure, RunnerOutput};
use crate::install::types::InstallStep;
use serde_json::Value;
use std::collections::HashMap;

const IMAGE: &str = "ghcr.io/openclaw/openclaw:latest";

pub fn run_step(
    step: &InstallStep,
    _artifacts: &HashMap<String, Value>,
) -> Result<RunnerOutput, RunnerFailure> {
    match step {
        InstallStep::Precheck => {
            let info = run_command("docker", &["info"])?;
            Ok(RunnerOutput {
                summary: "docker precheck completed".to_string(),
                details: info.stdout,
                commands: vec![info.command_line],
                artifacts: HashMap::new(),
            })
        }
        InstallStep::Install => {
            let pull = run_command("docker", &["pull", IMAGE])?;
            Ok(RunnerOutput {
                summary: "docker install completed".to_string(),
                details: if pull.stderr.is_empty() {
                    pull.stdout
                } else {
                    format!("{}\n{}", pull.stdout, pull.stderr)
                },
                commands: vec![pull.command_line],
                artifacts: HashMap::from([("docker_image".to_string(), Value::String(IMAGE.to_string()))]),
            })
        }
        InstallStep::Init => {
            let volume = run_command("docker", &["volume", "create", "clawpal-openclaw-config"])?;
            Ok(RunnerOutput {
                summary: "docker init completed".to_string(),
                details: volume.stdout,
                commands: vec![volume.command_line],
                artifacts: HashMap::from([(
                    "docker_volume".to_string(),
                    Value::String("clawpal-openclaw-config".to_string()),
                )]),
            })
        }
        InstallStep::Verify => {
            let inspect = run_command("docker", &["image", "inspect", IMAGE])?;
            Ok(RunnerOutput {
                summary: "docker verify completed".to_string(),
                details: "Docker image is present and inspect succeeded".to_string(),
                commands: vec![inspect.command_line],
                artifacts: HashMap::new(),
            })
        }
    }
}
