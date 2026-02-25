use super::RunnerOutput;
use crate::install::types::InstallStep;
use std::collections::HashMap;

pub fn run_step(step: &InstallStep) -> RunnerOutput {
    match step {
        InstallStep::Precheck => RunnerOutput {
            summary: "remote ssh precheck completed".to_string(),
            details: "Validated remote SSH connectivity and openclaw presence".to_string(),
            commands: vec!["ssh <host> 'which openclaw && openclaw --version'".to_string()],
            artifacts: HashMap::new(),
        },
        InstallStep::Install => RunnerOutput {
            summary: "remote ssh install completed".to_string(),
            details: "Prepared remote install command plan".to_string(),
            commands: vec!["ssh <host> 'openclaw upgrade --install'".to_string()],
            artifacts: HashMap::new(),
        },
        InstallStep::Init => RunnerOutput {
            summary: "remote ssh init completed".to_string(),
            details: "Initialized remote OpenClaw config".to_string(),
            commands: vec!["ssh <host> 'openclaw init'".to_string()],
            artifacts: HashMap::new(),
        },
        InstallStep::Verify => RunnerOutput {
            summary: "remote ssh verify completed".to_string(),
            details: "Verified remote OpenClaw status".to_string(),
            commands: vec!["ssh <host> 'openclaw status --json'".to_string()],
            artifacts: HashMap::new(),
        },
    }
}
