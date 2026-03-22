/**
 * Rescue bot and primary rescue type definitions.
 * Extracted from types.ts for readability.
 */

export type RescueBotAction = "set" | "activate" | "status" | "deactivate" | "unset";

export type RescueBotRuntimeState =
  | "unconfigured"
  | "configured_inactive"
  | "active"
  | "checking"
  | "error";

export interface RescueBotCommandResult {
  command: string[];
  output: {
    stdout: string;
    stderr: string;
    exitCode: number;
  };
}

export interface RescueBotManageResult {
  action: RescueBotAction;
  profile: string;
  mainPort: number;
  rescuePort: number;
  minRecommendedPort: number;
  configured: boolean;
  active: boolean;
  runtimeState: RescueBotRuntimeState;
  wasAlreadyConfigured: boolean;
  commands: RescueBotCommandResult[];
}

export interface RescuePrimaryCheckItem {
  id: string;
  title: string;
  ok: boolean;
  detail: string;
}

export interface RescuePrimaryIssue {
  id: string;
  code: string;
  severity: "error" | "warn" | "info";
  message: string;
  autoFixable: boolean;
  fixHint?: string;
  source: "rescue" | "primary";
}

export interface RescueDocHypothesis {
  title: string;
  reason: string;
  score: number;
}

export interface RescueDocCitation {
  url: string;
  section: string;
}

export interface RescuePrimarySummary {
  status: "healthy" | "degraded" | "broken" | "inactive";
  headline: string;
  recommendedAction: string;
  fixableIssueCount: number;
  selectedFixIssueIds: string[];
  rootCauseHypotheses?: RescueDocHypothesis[];
  fixSteps?: string[];
  confidence?: number;
  citations?: RescueDocCitation[];
  versionAwareness?: string;
}

export interface RescuePrimarySectionItem {
  id: string;
  label: string;
  status: "ok" | "warn" | "error" | "info" | "inactive";
  detail: string;
  autoFixable: boolean;
  issueId?: string | null;
}

export interface RescuePrimarySectionResult {
  key: "gateway" | "models" | "tools" | "agents" | "channels";
  title: string;
  status: "healthy" | "degraded" | "broken" | "inactive";
  summary: string;
  docsUrl: string;
  items: RescuePrimarySectionItem[];
  rootCauseHypotheses?: RescueDocHypothesis[];
  fixSteps?: string[];
  confidence?: number;
  citations?: RescueDocCitation[];
  versionAwareness?: string;
}

export interface RescuePrimaryDiagnosisResult {
  status: "healthy" | "degraded" | "broken" | "inactive";
  checkedAt: string;
  targetProfile: string;
  rescueProfile: string;
  rescueConfigured: boolean;
  rescuePort?: number;
  summary: RescuePrimarySummary;
  sections: RescuePrimarySectionResult[];
  checks: RescuePrimaryCheckItem[];
  issues: RescuePrimaryIssue[];
}

export interface RescuePrimaryRepairStep {
  id: string;
  title: string;
  ok: boolean;
  detail: string;
  command?: string[];
}

export interface RescuePrimaryPendingAction {
  kind: "tempProviderSetup";
  reason: string;
  tempProviderProfileId?: string | null;
}

export interface RescuePrimaryRepairResult {
  status: "completed" | "needsTempProviderSetup";
  attemptedAt: string;
  targetProfile: string;
  rescueProfile: string;
  selectedIssueIds: string[];
  appliedIssueIds: string[];
  skippedIssueIds: string[];
  failedIssueIds: string[];
  pendingAction?: RescuePrimaryPendingAction | null;
  steps: RescuePrimaryRepairStep[];
  before: RescuePrimaryDiagnosisResult;
  after: RescuePrimaryDiagnosisResult;
}
