import type { SshDiagnosticReport } from "./ssh-types";
export type {
  SftpEntry,
  SshCommandError,
  SshConfigHostSuggestion,
  SshConnectionBottleneckStage,
  SshConnectionProbePhase,
  SshConnectionProbeStatus,
  SshConnectionProfile,
  SshConnectionQuality,
  SshConnectionStageKey,
  SshConnectionStageMetric,
  SshConnectionStageStatus,
  SshDiagnosticReport,
  SshDiagnosticStatus,
  SshErrorCode,
  SshEvidence,
  SshExecResult,
  SshHost,
  SshIntent,
  SshProbeProgressEvent,
  SshRepairAction,
  SshStage,
  SshTransferStats,
} from "./ssh-types";

export type Severity = "low" | "medium" | "high";

export interface ChannelNode {
  path: string;
  channelType: string | null;
  mode: string | null;
  allowlist: string[];
  model: string | null;
  hasModelField: boolean;
  displayName: string | null;
  nameStatus: string | null;
}

export interface DiscordGuildChannel {
  guildId: string;
  guildName: string;
  channelId: string;
  channelName: string;
  defaultAgentId?: string;
}

export interface RecipeParam {
  id: string;
  label: string;
  type: "string" | "number" | "boolean" | "textarea" | "discord_guild" | "discord_channel" | "model_profile" | "agent";
  required: boolean;
  pattern?: string;
  minLength?: number;
  maxLength?: number;
  placeholder?: string;
  dependsOn?: string;
  defaultValue?: string;
}

export interface RecipeStep {
  action: string;
  label: string;
  args: Record<string, unknown>;
}

export interface Recipe {
  id: string;
  name: string;
  description: string;
  version: string;
  tags: string[];
  difficulty: "easy" | "normal" | "advanced";
  params: RecipeParam[];
  steps: RecipeStep[];
}

export interface ChangeItem {
  path: string;
  op: string;
  risk: string;
  reason?: string;
}

export interface PreviewResult {
  recipeId: string;
  diff: string;
  configBefore: string;
  configAfter: string;
  changes: ChangeItem[];
  overwritesExisting: boolean;
  canRollback: boolean;
  impactLevel: string;
  warnings: string[];
}

export interface ApplyResult {
  ok: boolean;
  snapshotId?: string;
  configPath: string;
  backupPath?: string;
  warnings: string[];
  errors: string[];
}

export interface SystemStatus {
  healthy: boolean | null;
  configPath: string;
  openclawDir: string;
  clawpalDir: string;
  openclawVersion: string;
  activeAgents: number;
  snapshots: number;
  openclawUpdate?: {
    installedVersion: string;
    latestVersion?: string;
    upgradeAvailable: boolean;
    channel?: string;
    details?: string;
    source: string;
    checkedAt: string;
  };
  channels: {
    configuredChannels: number;
    channelModelOverrides: number;
    channelExamples: string[];
  };
  models: {
    globalDefaultModel?: string;
    agentOverrides: string[];
    channelOverrides: string[];
  };
  memory: {
    fileCount: number;
    totalBytes: number;
    files: { path: string; sizeBytes: number }[];
  };
  sessions: {
    totalSessionFiles: number;
    totalArchiveFiles: number;
    totalBytes: number;
    byAgent: { agent: string; sessionFiles: number; archiveFiles: number; totalBytes: number }[];
  };
}

export interface SessionFile {
  path: string;
  relativePath: string;
  agent: string;
  kind: "sessions" | "archive";
  sizeBytes: number;
}

export interface SessionAnalysis {
  agent: string;
  sessionId: string;
  filePath: string;
  sizeBytes: number;
  messageCount: number;
  userMessageCount: number;
  assistantMessageCount: number;
  lastActivity: string | null;
  ageDays: number;
  totalTokens: number;
  model: string | null;
  category: "empty" | "low_value" | "valuable";
  kind: string;
}

export interface AgentSessionAnalysis {
  agent: string;
  totalFiles: number;
  totalSizeBytes: number;
  emptyCount: number;
  lowValueCount: number;
  valuableCount: number;
  sessions: SessionAnalysis[];
}

export interface ModelProfile {
  id: string;
  name: string;
  provider: string;
  model: string;
  authRef: string;
  apiKey?: string;
  baseUrl?: string;
  description?: string;
  enabled: boolean;
}

export interface ModelCatalogModel {
  id: string;
  name?: string;
}

export interface ModelCatalogProvider {
  provider: string;
  baseUrl?: string;
  models: ModelCatalogModel[];
}

export interface ProviderAuthSuggestion {
  authRef: string | null;
  hasKey: boolean;
  source: string;
}

export interface ResolvedApiKey {
  profileId: string;
  maskedKey: string;
  credentialKind?: "oauth" | "env_ref" | "manual" | "unset";
  authRef?: string | null;
  resolved?: boolean;
}

export interface RemoteAuthSyncResult {
  totalRemoteProfiles: number;
  syncedProfiles: number;
  createdProfiles: number;
  updatedProfiles: number;
  resolvedKeys: number;
  unresolvedKeys: number;
  failedKeyResolves: number;
}

export interface ProfilePushResult {
  requestedProfiles: number;
  pushedProfiles: number;
  writtenModelEntries: number;
  writtenAuthEntries: number;
  blockedProfiles: number;
}

export interface RelatedSecretPushResult {
  totalRelatedProviders: number;
  resolvedSecrets: number;
  writtenSecrets: number;
  skippedProviders: number;
  failedProviders: number;
}

export interface AppPreferences {
  showSshTransferSpeedUi: boolean;
}


export type BugReportBackend = "sentry";
export type BugReportSeverity = "info" | "warn" | "error" | "critical";

export interface BugReportSettings {
  enabled: boolean;
  backend: BugReportBackend;
  endpoint: string | null;
  severityThreshold: BugReportSeverity;
  maxReportsPerHour: number;
}

export interface BugReportStats {
  sessionId: string;
  totalSent: number;
  sentLastHour: number;
  droppedRateLimited: number;
  sendFailures: number;
  lastSentAt: string | null;
  persistedPending: number;
  deadLetterCount: number;
}

export interface HistoryItem {
  id: string;
  recipeId?: string;
  createdAt: string;
  source: string;
  canRollback: boolean;
  rollbackOf?: string;
}

export interface DoctorIssue {
  id: string;
  code: string;
  severity: "error" | "warn" | "info";
  message: string;
  autoFixable: boolean;
  fixHint?: string;
}

export interface DoctorReport {
  ok: boolean;
  score: number;
  issues: DoctorIssue[];
}

export interface GuidanceAction {
  label: string;
  actionType: "inline_fix" | "doctor_handoff";
  tool?: string;
  args?: string;
  invokeType?: string;
  context?: string;
}

export interface PrecheckIssue {
  code: string;
  severity: "error" | "warn";
  message: string;
  autoFixable: boolean;
}

export interface AgentOverview {
  id: string;
  name?: string;
  emoji?: string;
  model: string | null;
  channels: string[];
  online: boolean;
  workspace?: string;
}

export interface InstanceStatus {
  healthy: boolean | null;
  activeAgents: number;
  globalDefaultModel?: string;
  fallbackModels?: string[];
  sshDiagnostic?: SshDiagnosticReport | null;
}





export interface StatusExtra {
  openclawVersion?: string;
  duplicateInstalls?: string[];
}

export interface InstanceConfigSnapshot {
  globalDefaultModel?: string;
  fallbackModels: string[];
  agents: AgentOverview[];
}

export interface InstanceRuntimeSnapshot {
  status: InstanceStatus;
  agents: AgentOverview[];
  globalDefaultModel?: string;
  fallbackModels: string[];
}

export interface ChannelsConfigSnapshot {
  channels: ChannelNode[];
  bindings: Binding[];
}

export interface ChannelsRuntimeSnapshot {
  channels: ChannelNode[];
  bindings: Binding[];
  agents: AgentOverview[];
}



export interface Binding {
  agentId: string;
  match: { channel: string; peer?: { id: string; kind: string } };
}

export interface BackupInfo {
  name: string;
  path: string;
  createdAt: string;
  sizeBytes: number;
}











export interface DockerInstance {
  id: string;
  label: string;
  projectDir?: string;
  openclawHome?: string;
  clawpalDataDir?: string;
}

export interface RegisteredInstance {
  id: string;
  instanceType: "local" | "docker" | "remote_ssh";
  label: string;
  openclawHome?: string | null;
  clawpalDataDir?: string | null;
}

export interface DiscoveredInstance {
  id: string;
  instanceType: string;
  label: string;
  homePath: string;
  source: string;
  containerName?: string;
  alreadyRegistered: boolean;
}

















// Cron









// Command Queue

export interface PendingCommand {
  id: string;
  label: string;
  command: string[];
  createdAt: string;
}

export interface PreviewQueueResult {
  commands: PendingCommand[];
  configBefore: string;
  configAfter: string;
  warnings: string[];
  errors: string[];
}

// Doctor Agent

export interface DoctorInvoke {
  id: string;
  command: string;
  args: Record<string, unknown>;
  type: "read" | "write";
}

export interface DiagnosisCitation {
  url: string;
  section?: string;
}

export interface DiagnosisReportItem {
  problem: string;
  severity: "error" | "warn" | "info";
  fix_options: string[];
  root_cause_hypothesis?: string;
  fix_steps?: string[];
  confidence?: number;
  citations?: DiagnosisCitation[];
  version_awareness?: string;
  action?: { tool: string; args: string; instance?: string; reason?: string };
}

export interface DoctorChatMessage {
  id: string;
  role: "assistant" | "user" | "tool-call" | "tool-result";
  content: string;
  invoke?: DoctorInvoke;
  invokeResult?: unknown;
  invokeId?: string;
  status?: "pending" | "approved" | "rejected" | "auto";
  diagnosisReport?: { items: DiagnosisReportItem[] };
  /** Epoch milliseconds when the message was created. */
  timestamp?: number;
}

export interface ApplyQueueResult {
  ok: boolean;
  appliedCount: number;
  totalCount: number;
  error: string | null;
  rolledBack: boolean;
}














export type {
  RescueBotAction,
  RescueBotRuntimeState,
  RescueBotCommandResult,
  RescueBotManageResult,
  RescuePrimaryCheckItem,
  RescuePrimaryIssue,
  RescueDocHypothesis,
  RescueDocCitation,
  RescuePrimarySummary,
  RescuePrimarySectionItem,
  RescuePrimarySectionResult,
  RescuePrimaryDiagnosisResult,
  RescuePrimaryRepairStep,
  RescuePrimaryPendingAction,
  RescuePrimaryRepairResult,
} from "./rescue-types";

export type {
  InstallMethod,
  InstallState,
  InstallStep,
  InstallLogEntry,
  InstallSession,
  InstallStepResult,
  InstallMethodCapability,
  InstallOrchestratorDecision,
  InstallUiAction,
  InstallTargetDecision,
  EnsureAccessResult,
  RecordInstallExperienceResult,
} from "./install-types";

export type {
  CronConfigSnapshot,
  CronRuntimeSnapshot,
  WatchdogJobStatus,
  CronSchedule,
  CronJobState,
  CronJobDelivery,
  CronJob,
  CronRun,
  WatchdogJobState,
  WatchdogStatus,
} from "./cron-types";
