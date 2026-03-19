import type { SshDiagnosticReport } from "./ssh-types";
/**
 * Installation workflow type definitions.
 * Extracted from types.ts for readability.
 */

export type InstallMethod = "local" | "wsl2" | "docker" | "remote_ssh";

export type InstallState =
  | "idle"
  | "selected_method"
  | "precheck_running"
  | "precheck_failed"
  | "precheck_passed"
  | "install_running"
  | "install_failed"
  | "install_passed"
  | "init_running"
  | "init_failed"
  | "init_passed"
  | "verify_running"
  | "verify_failed"
  | "ready";

export type InstallStep = "precheck" | "install" | "init" | "verify";

export interface InstallLogEntry {
  at: string;
  level: string;
  message: string;
}

export interface InstallSession {
  id: string;
  method: InstallMethod;
  state: InstallState;
  current_step: InstallStep | null;
  logs: InstallLogEntry[];
  artifacts: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface InstallStepResult {
  ok: boolean;
  summary: string;
  details: string;
  commands: string[];
  artifacts: Record<string, unknown>;
  next_step: string | null;
  error_code: string | null;
  ssh_diagnostic?: SshDiagnosticReport | null;
}

export interface InstallMethodCapability {
  method: InstallMethod;
  available: boolean;
  hint: string | null;
}

export interface InstallOrchestratorDecision {
  step: string | null;
  reason: string;
  source: string;
  errorCode?: string | null;
  actionHint?: string | null;
}

export interface InstallUiAction {
  id: string;
  kind: string;
  label: string;
  payload?: Record<string, unknown>;
}

export interface InstallTargetDecision {
  method: InstallMethod | null;
  reason: string;
  source: string;
  requiresSshHost: boolean;
  requiredFields?: string[];
  uiActions?: InstallUiAction[];
  errorCode?: string | null;
  actionHint?: string | null;
}

export interface EnsureAccessResult {
  instanceId: string;
  transport: string;
  workingChain: string[];
  usedLegacyFallback: boolean;
  profileReused: boolean;
}

export interface RecordInstallExperienceResult {
  saved: boolean;
  totalCount: number;
}

