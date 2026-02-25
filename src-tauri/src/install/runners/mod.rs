use crate::install::types::{InstallMethod, InstallStep};
use serde_json::Value;
use std::collections::HashMap;

pub mod docker;
pub mod local;
pub mod remote_ssh;
pub mod wsl2;

#[derive(Clone, Debug)]
pub struct RunnerOutput {
    pub summary: String,
    pub details: String,
    pub commands: Vec<String>,
    pub artifacts: HashMap<String, Value>,
}

pub fn run_step(method: &InstallMethod, step: &InstallStep) -> RunnerOutput {
    match method {
        InstallMethod::Local => local::run_step(step),
        InstallMethod::Wsl2 => wsl2::run_step(step),
        InstallMethod::Docker => docker::run_step(step),
        InstallMethod::RemoteSsh => remote_ssh::run_step(step),
    }
}
