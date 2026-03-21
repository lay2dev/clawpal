use serde::{Deserialize, Serialize};

use crate::openclaw_doc_resolver::{DocCitation, RootCauseHypothesis};
use clawpal_core::ssh::diagnostic::SshDiagnosticReport;

pub type ModelProfile = clawpal_core::profile::ModelProfile;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub healthy: bool,
    pub config_path: String,
    pub openclaw_dir: String,
    pub clawpal_dir: String,
    pub openclaw_version: String,
    pub active_agents: u32,
    pub snapshots: usize,
    pub channels: ChannelSummary,
    pub models: ModelSummary,
    pub memory: MemorySummary,
    pub sessions: SessionSummary,
    pub openclaw_update: OpenclawUpdateCheck,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenclawUpdateCheck {
    pub installed_version: String,
    pub latest_version: Option<String>,
    pub upgrade_available: bool,
    pub channel: Option<String>,
    pub details: Option<String>,
    pub source: String,
    pub checked_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogProviderCache {
    pub cli_version: String,
    pub updated_at: u64,
    pub providers: Vec<ModelCatalogProvider>,
    pub source: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenclawCommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl From<crate::cli_runner::CliOutput> for OpenclawCommandOutput {
    fn from(value: crate::cli_runner::CliOutput) -> Self {
        Self {
            stdout: value.stdout,
            stderr: value.stderr,
            exit_code: value.exit_code,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RescueBotCommandResult {
    pub command: Vec<String>,
    pub output: OpenclawCommandOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RescueBotManageResult {
    pub action: String,
    pub profile: String,
    pub main_port: u16,
    pub rescue_port: u16,
    pub min_recommended_port: u16,
    pub configured: bool,
    pub active: bool,
    pub runtime_state: String,
    pub was_already_configured: bool,
    pub commands: Vec<RescueBotCommandResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RescuePrimaryCheckItem {
    pub id: String,
    pub title: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RescuePrimaryIssue {
    pub id: String,
    pub code: String,
    pub severity: String,
    pub message: String,
    pub auto_fixable: bool,
    pub fix_hint: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RescuePrimaryDiagnosisResult {
    pub status: String,
    pub checked_at: String,
    pub target_profile: String,
    pub rescue_profile: String,
    pub rescue_configured: bool,
    pub rescue_port: Option<u16>,
    pub summary: RescuePrimarySummary,
    pub sections: Vec<RescuePrimarySectionResult>,
    pub checks: Vec<RescuePrimaryCheckItem>,
    pub issues: Vec<RescuePrimaryIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RescuePrimarySummary {
    pub status: String,
    pub headline: String,
    pub recommended_action: String,
    pub fixable_issue_count: usize,
    pub selected_fix_issue_ids: Vec<String>,
    #[serde(default)]
    pub root_cause_hypotheses: Vec<RootCauseHypothesis>,
    #[serde(default)]
    pub fix_steps: Vec<String>,
    pub confidence: Option<f32>,
    #[serde(default)]
    pub citations: Vec<DocCitation>,
    pub version_awareness: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RescuePrimarySectionResult {
    pub key: String,
    pub title: String,
    pub status: String,
    pub summary: String,
    pub docs_url: String,
    pub items: Vec<RescuePrimarySectionItem>,
    #[serde(default)]
    pub root_cause_hypotheses: Vec<RootCauseHypothesis>,
    #[serde(default)]
    pub fix_steps: Vec<String>,
    pub confidence: Option<f32>,
    #[serde(default)]
    pub citations: Vec<DocCitation>,
    pub version_awareness: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RescuePrimarySectionItem {
    pub id: String,
    pub label: String,
    pub status: String,
    pub detail: String,
    pub auto_fixable: bool,
    pub issue_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RescuePrimaryRepairStep {
    pub id: String,
    pub title: String,
    pub ok: bool,
    pub detail: String,
    pub command: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RescuePrimaryPendingAction {
    pub kind: String,
    pub reason: String,
    pub temp_provider_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RescuePrimaryRepairResult {
    pub status: String,
    pub attempted_at: String,
    pub target_profile: String,
    pub rescue_profile: String,
    pub selected_issue_ids: Vec<String>,
    pub applied_issue_ids: Vec<String>,
    pub skipped_issue_ids: Vec<String>,
    pub failed_issue_ids: Vec<String>,
    pub pending_action: Option<RescuePrimaryPendingAction>,
    pub steps: Vec<RescuePrimaryRepairStep>,
    pub before: RescuePrimaryDiagnosisResult,
    pub after: RescuePrimaryDiagnosisResult,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractModelProfilesResult {
    pub created: usize,
    pub reused: usize,
    pub skipped_invalid: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractModelProfileEntry {
    pub provider: String,
    pub model: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenclawUpdateCache {
    pub checked_at: u64,
    pub latest_version: Option<String>,
    pub channel: Option<String>,
    pub details: Option<String>,
    pub source: String,
    pub installed_version: Option<String>,
    pub ttl_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSummary {
    pub global_default_model: Option<String>,
    pub agent_overrides: Vec<String>,
    pub channel_overrides: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSummary {
    pub configured_channels: usize,
    pub channel_model_overrides: usize,
    pub channel_examples: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFileSummary {
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummary {
    pub file_count: usize,
    pub total_bytes: u64,
    pub files: Vec<MemoryFileSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionSummary {
    pub agent: String,
    pub session_files: usize,
    pub archive_files: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFile {
    pub path: String,
    pub relative_path: String,
    pub agent: String,
    pub kind: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAnalysis {
    pub agent: String,
    pub session_id: String,
    pub file_path: String,
    pub size_bytes: u64,
    pub message_count: usize,
    pub user_message_count: usize,
    pub assistant_message_count: usize,
    pub last_activity: Option<String>,
    pub age_days: f64,
    pub total_tokens: u64,
    pub model: Option<String>,
    pub category: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionAnalysis {
    pub agent: String,
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub empty_count: usize,
    pub low_value_count: usize,
    pub valuable_count: usize,
    pub sessions: Vec<SessionAnalysis>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub total_session_files: usize,
    pub total_archive_files: usize,
    pub total_bytes: u64,
    pub by_agent: Vec<AgentSessionSummary>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogModel {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogProvider {
    pub provider: String,
    pub base_url: Option<String>,
    pub models: Vec<ModelCatalogModel>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelNode {
    pub path: String,
    pub channel_type: Option<String>,
    pub mode: Option<String>,
    pub allowlist: Vec<String>,
    pub model: Option<String>,
    pub has_model_field: bool,
    pub display_name: Option<String>,
    pub name_status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordGuildChannel {
    pub guild_id: String,
    pub guild_name: String,
    pub channel_id: String,
    pub channel_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_agent_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAuthSuggestion {
    pub auth_ref: Option<String>,
    pub has_key: bool,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelBinding {
    pub scope: String,
    pub scope_id: String,
    pub model_profile_id: Option<String>,
    pub model_value: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: String,
    pub recipe_id: Option<String>,
    pub created_at: String,
    pub source: String,
    pub can_rollback: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_of: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub items: Vec<HistoryItem>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixResult {
    pub ok: bool,
    pub applied: Vec<String>,
    pub remaining_issues: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentOverview {
    pub id: String,
    pub name: Option<String>,
    pub emoji: Option<String>,
    pub model: Option<String>,
    pub channels: Vec<String>,
    pub online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusLight {
    pub healthy: bool,
    pub active_agents: u32,
    pub global_default_model: Option<String>,
    pub fallback_models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_diagnostic: Option<SshDiagnosticReport>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusExtra {
    pub openclaw_version: Option<String>,
    pub duplicate_installs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshBottleneck {
    pub stage: String,
    pub latency_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConnectionStage {
    pub key: String,
    pub latency_ms: u64,
    pub status: String,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConnectionProfile {
    pub probe_status: String,
    pub reused_existing_connection: bool,
    pub status: StatusLight,
    pub connect_latency_ms: u64,
    pub gateway_latency_ms: u64,
    pub config_latency_ms: u64,
    pub agents_latency_ms: u64,
    pub version_latency_ms: u64,
    pub total_latency_ms: u64,
    pub quality: String,
    pub quality_score: u8,
    pub bottleneck: SshBottleneck,
    pub stages: Vec<SshConnectionStage>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedApiKey {
    pub profile_id: String,
    pub masked_key: String,
    pub credential_kind: ResolvedCredentialKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_ref: Option<String>,
    pub resolved: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedCredentialKind {
    OAuth,
    EnvRef,
    Manual,
    Unset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InternalAuthKind {
    ApiKey,
    Authorization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolvedCredentialSource {
    ExplicitAuthRef,
    ManualApiKey,
    ProviderFallbackAuthRef,
    ProviderEnvVar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InternalProviderCredential {
    pub secret: String,
    pub kind: InternalAuthKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub name: String,
    pub path: String,
    pub created_at: String,
    pub size_bytes: u64,
}
