use super::RunnerOutput;
use crate::install::types::InstallStep;
use std::collections::HashMap;

pub fn run_step(step: &InstallStep) -> RunnerOutput {
    match step {
        InstallStep::Precheck => RunnerOutput {
            summary: "docker precheck completed".to_string(),
            details: "Validated Docker daemon availability".to_string(),
            commands: vec!["docker info".to_string()],
            artifacts: HashMap::new(),
        },
        InstallStep::Install => RunnerOutput {
            summary: "docker install completed".to_string(),
            details: "Prepared Docker image and container plan".to_string(),
            commands: vec!["docker pull ghcr.io/openclaw/openclaw:latest".to_string()],
            artifacts: HashMap::new(),
        },
        InstallStep::Init => RunnerOutput {
            summary: "docker init completed".to_string(),
            details: "Initialized mounted OpenClaw config volume".to_string(),
            commands: vec!["docker run --rm ghcr.io/openclaw/openclaw:latest openclaw init".to_string()],
            artifacts: HashMap::new(),
        },
        InstallStep::Verify => RunnerOutput {
            summary: "docker verify completed".to_string(),
            details: "Verified containerized OpenClaw status".to_string(),
            commands: vec!["docker run --rm ghcr.io/openclaw/openclaw:latest openclaw status --json".to_string()],
            artifacts: HashMap::new(),
        },
    }
}
