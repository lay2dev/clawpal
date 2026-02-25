use super::RunnerOutput;
use crate::install::types::InstallStep;
use std::collections::HashMap;

pub fn run_step(step: &InstallStep) -> RunnerOutput {
    match step {
        InstallStep::Precheck => RunnerOutput {
            summary: "local precheck completed".to_string(),
            details: "Checked local shell environment and openclaw availability".to_string(),
            commands: vec!["which openclaw".to_string(), "openclaw --version".to_string()],
            artifacts: HashMap::new(),
        },
        InstallStep::Install => RunnerOutput {
            summary: "local install completed".to_string(),
            details: "Prepared local install command plan".to_string(),
            commands: vec!["openclaw upgrade --install".to_string()],
            artifacts: HashMap::new(),
        },
        InstallStep::Init => RunnerOutput {
            summary: "local init completed".to_string(),
            details: "Initialized local OpenClaw config directory".to_string(),
            commands: vec!["openclaw init".to_string()],
            artifacts: HashMap::new(),
        },
        InstallStep::Verify => RunnerOutput {
            summary: "local verify completed".to_string(),
            details: "Verified local OpenClaw status".to_string(),
            commands: vec!["openclaw status --json".to_string()],
            artifacts: HashMap::new(),
        },
    }
}
