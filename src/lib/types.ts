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
  resolutionWarning?: string;
  guildResolutionWarning?: string;
  channelResolutionWarning?: string;
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
  options?: Array<{ value: string; label: string }>;
}

export interface RecipeStep {
  action: string;
  label: string;
  args: Record<string, unknown>;
}

export interface RecipePresentation {
  resultSummary?: string;
}

export interface Recipe {
  id: string;
  name: string;
  description: string;
  version: string;
  tags: string[];
  difficulty: "easy" | "normal" | "advanced";
  presentation?: RecipePresentation;
  params: RecipeParam[];
  steps: RecipeStep[];
}

export interface RecipeWorkspaceEntry {
  slug: string;
  path: string;
  recipeId?: string;
  version?: string;
  sourceKind?: "bundled" | "localImport" | "remoteUrl";
  bundledVersion?: string;
  bundledState?:
    | "missing"
    | "upToDate"
    | "updateAvailable"
    | "localModified"
    | "conflictedUpdate";
  trustLevel: "trusted" | "caution" | "untrusted";
  riskLevel: "low" | "medium" | "high";
  approvalRequired: boolean;
}

export interface RecipeActionCatalogEntry {
  kind: string;
  title: string;
  group: string;
  category: string;
  backend: string;
  description: string;
  readOnly: boolean;
  interactive: boolean;
  runnerSupported: boolean;
  recommended: boolean;
  cliCommand?: string;
  legacyAliasOf?: string;
  capabilities: string[];
  resourceKinds: string[];
}

export interface RecipeSourceSaveResult {
  slug: string;
  path: string;
}

export interface ImportedRecipe {
  slug: string;
  recipeId: string;
  path: string;
}

export interface SkippedRecipeImport {
  recipeDir: string;
  reason: string;
}

export interface RecipeLibraryImportResult {
  imported: ImportedRecipe[];
  skipped: SkippedRecipeImport[];
  warnings: string[];
}

export type RecipeImportSourceKind =
  | "localFile"
  | "localRecipeDirectory"
  | "localRecipeLibrary"
  | "remoteUrl";

export interface RecipeImportConflict {
  slug: string;
  recipeId: string;
  path: string;
}

export interface SkippedRecipeSourceImport {
  source: string;
  reason: string;
}

export interface RecipeSourceImportResult {
  sourceKind?: RecipeImportSourceKind | null;
  imported: ImportedRecipe[];
  skipped: SkippedRecipeSourceImport[];
  warnings: string[];
  conflicts: RecipeImportConflict[];
}

export interface RecipeSourceDiagnostic {
  category: string;
  severity: string;
  recipeId?: string;
  path?: string;
  message: string;
}

export interface RecipeSourceDiagnostics {
  errors: RecipeSourceDiagnostic[];
  warnings: RecipeSourceDiagnostic[];
}

export type RecipeEditorOrigin = "builtin" | "workspace" | "external";

export interface RecipeStudioDraft {
  recipeId: string;
  recipeName: string;
  source: string;
  origin: RecipeEditorOrigin;
  workspaceSlug?: string;
}

export interface RecipeEditorActionRow {
  kind: string;
  name: string;
  argsText: string;
}

export interface RecipeEditorModel {
  id: string;
  name: string;
  description: string;
  version: string;
  tagsText: string;
  difficulty: Recipe["difficulty"];
  params: RecipeParam[];
  steps: RecipeStep[];
  actionRows: RecipeEditorActionRow[];
  bundleCapabilities: string[];
  bundleResources: string[];
  executionKind: RecipeExecutionKind;
  sourceDocument: unknown;
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

export type RecipeExecutionKind = "job" | "service" | "schedule" | "attachment";

export interface RecipeBundle {
  apiVersion: string;
  kind: string;
  metadata: {
    name?: string;
    version?: string;
    description?: string;
  };
  compatibility: {
    minRunnerVersion?: string;
    targetPlatforms?: string[];
  };
  inputs: Record<string, unknown>[];
  capabilities: {
    allowed: string[];
  };
  resources: {
    supportedKinds: string[];
  };
  execution: {
    supportedKinds: RecipeExecutionKind[];
  };
  runner: {
    name?: string;
    version?: string;
  };
  outputs: Record<string, unknown>[];
}

export interface ExecutionResourceClaim {
  kind: string;
  id?: string;
  target?: string;
  path?: string;
}

export interface ExecutionSecretBinding {
  id: string;
  source: string;
  mount?: string;
}

export interface ExecutionSpec {
  apiVersion: string;
  kind: string;
  metadata: {
    name?: string;
    digest?: string;
  };
  source: Record<string, unknown>;
  target: Record<string, unknown>;
  execution: {
    kind: RecipeExecutionKind;
  };
  capabilities: {
    usedCapabilities: string[];
  };
  resources: {
    claims: ExecutionResourceClaim[];
  };
  secrets: {
    bindings: ExecutionSecretBinding[];
  };
  desiredState: Record<string, unknown>;
  actions: Record<string, unknown>[];
  outputs: Record<string, unknown>[];
}

export interface RecipePlanSummary {
  recipeId: string;
  recipeName: string;
  executionKind: RecipeExecutionKind;
  actionCount: number;
  skippedStepCount: number;
}

export interface RecipePlan {
  summary: RecipePlanSummary;
  usedCapabilities: string[];
  concreteClaims: ExecutionResourceClaim[];
  executionSpecDigest: string;
  executionSpec: ExecutionSpec;
  warnings: string[];
}

export type RecipeSourceOrigin = "saved" | "draft";

export interface ExecuteRecipeRequest {
  spec: ExecutionSpec;
  sourceOrigin?: RecipeSourceOrigin;
  sourceText?: string;
  workspaceSlug?: string;
  activitySessionId?: string;
  planningAuditTrail?: RecipeRuntimeAuditEntry[];
}

export interface ExecuteRecipeResult {
  runId: string;
  instanceId: string;
  summary: string;
  warnings: string[];
  auditTrail?: RecipeRuntimeAuditEntry[];
}

export interface RecipeRuntimeArtifact {
  id: string;
  kind: string;
  label: string;
  path?: string;
}

export interface RecipeRuntimeAuditEntry {
  id: string;
  phase: string;
  kind: string;
  label: string;
  status: "started" | "succeeded" | "failed";
  sideEffect: boolean;
  startedAt: string;
  finishedAt?: string;
  target?: string;
  displayCommand?: string;
  exitCode?: number;
  stdoutSummary?: string;
  stderrSummary?: string;
  details?: string;
}

export interface CookActivityEvent extends RecipeRuntimeAuditEntry {
  sessionId: string;
  runId?: string;
  instanceId: string;
}

export interface RecipeRuntimeRun {
  id: string;
  instanceId: string;
  recipeId: string;
  executionKind: string;
  runner: string;
  status: string;
  summary: string;
  startedAt: string;
  finishedAt?: string;
  artifacts: RecipeRuntimeArtifact[];
  resourceClaims: ExecutionResourceClaim[];
  warnings: string[];
  sourceOrigin?: string;
  sourceDigest?: string;
  workspacePath?: string;
  auditTrail?: RecipeRuntimeAuditEntry[];
}

export interface RecipeRuntimeInstance {
  id: string;
  recipeId: string;
  executionKind: string;
  runner: string;
  status: string;
  lastRunId?: string;
  updatedAt: string;
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

export interface SessionAnalysisChunkEvent {
  handleId: string;
  agent: string;
  sessions: SessionAnalysis[];
  totalFiles: number;
  totalSizeBytes: number;
  emptyCount: number;
  lowValueCount: number;
  valuableCount: number;
  done: boolean;
}

export interface SessionStreamDoneEvent {
  handleId: string;
  totalAgents: number;
  totalSessions: number;
  cancelled: boolean;
}

export interface SessionPreviewMessage {
  role: string;
  content: string;
}

export interface SessionPreviewPageEvent {
  handleId: string;
  page: number;
  messages: SessionPreviewMessage[];
  totalMessages: number;
}

export interface SessionPreviewDoneEvent {
  handleId: string;
  totalMessages: number;
  cancelled: boolean;
}

export interface SessionStreamErrorEvent {
  handleId: string;
  error: string;
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
  runId?: string;
  rollbackOf?: string;
  artifacts?: RecipeRuntimeArtifact[];
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

export interface BackupProgressEvent {
  handleId: string;
  phase: string;
  filesCopied: number;
  bytesCopied: number;
  currentPath?: string | null;
}

export interface BackupDoneEvent {
  handleId: string;
  info: BackupInfo;
}

export interface BackupErrorEvent {
  handleId: string;
  error: string;
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

export type {
  ApplyQueueResult,
  DiagnosisCitation,
  DiagnosisReportItem,
  DoctorChatMessage,
  DoctorInvoke,
  DoctorIssue,
  DoctorReport,
  PendingCommand,
  PreviewQueueResult,
} from "./doctor-types";
