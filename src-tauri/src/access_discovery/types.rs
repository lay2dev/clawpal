use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub probe_id: String,
    pub command: String,
    pub ok: bool,
    pub summary: String,
    pub elapsed_ms: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityProfile {
    pub instance_id: String,
    pub transport: String,
    pub probes: Vec<ProbeResult>,
    pub working_chain: Vec<String>,
    pub env_contract: BTreeMap<String, String>,
    pub verified_at: u64,
    pub ttl_secs: u64,
}

impl CapabilityProfile {
    pub fn example_local(instance_id: &str) -> Self {
        Self {
            instance_id: instance_id.to_string(),
            transport: "local".to_string(),
            probes: vec![ProbeResult {
                probe_id: "probe-version".to_string(),
                command: "openclaw --version".to_string(),
                ok: true,
                summary: "openclaw command available".to_string(),
                elapsed_ms: 12,
            }],
            working_chain: vec!["openclaw".to_string(), "--version".to_string()],
            env_contract: BTreeMap::new(),
            verified_at: 0,
            ttl_secs: 3600,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionExperience {
    pub instance_id: String,
    pub goal: String,
    pub transport: String,
    pub method: String,
    pub commands: Vec<String>,
    pub successful_chain: Vec<String>,
    pub recorded_at: u64,
}
