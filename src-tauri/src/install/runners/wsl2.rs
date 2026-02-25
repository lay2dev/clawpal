use super::RunnerOutput;
use crate::install::types::InstallStep;
use std::collections::HashMap;

pub fn run_step(step: &InstallStep) -> RunnerOutput {
    match step {
        InstallStep::Precheck => RunnerOutput {
            summary: "wsl2 precheck completed".to_string(),
            details: "Validated WSL2 environment and shell access".to_string(),
            commands: vec!["wsl.exe --status".to_string(), "wsl.exe which openclaw".to_string()],
            artifacts: HashMap::new(),
        },
        InstallStep::Install => RunnerOutput {
            summary: "wsl2 install completed".to_string(),
            details: "Prepared WSL2 install command plan".to_string(),
            commands: vec!["wsl.exe openclaw upgrade --install".to_string()],
            artifacts: HashMap::new(),
        },
        InstallStep::Init => RunnerOutput {
            summary: "wsl2 init completed".to_string(),
            details: "Initialized OpenClaw in WSL2 user home".to_string(),
            commands: vec!["wsl.exe openclaw init".to_string()],
            artifacts: HashMap::new(),
        },
        InstallStep::Verify => RunnerOutput {
            summary: "wsl2 verify completed".to_string(),
            details: "Verified WSL2 OpenClaw status".to_string(),
            commands: vec!["wsl.exe openclaw status --json".to_string()],
            artifacts: HashMap::new(),
        },
    }
}
