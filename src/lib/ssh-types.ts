import type { InstanceStatus } from "./types";
/**
 * SSH-related type definitions.
 * Extracted from types.ts for readability.
 */

export interface SshTransferStats {
  hostId: string;
  uploadBytesPerSec: number;
  downloadBytesPerSec: number;
  totalUploadBytes: number;
  totalDownloadBytes: number;
  updatedAtMs: number;
}

export type SshConnectionQuality = "excellent" | "good" | "fair" | "poor" | "unknown";

export type SshConnectionBottleneckStage = "connect" | "gateway" | "config" | "agents" | "version" | "other";

export type SshConnectionProbeStatus = "success" | "failed" | "interactive_required";

export type SshConnectionStageKey = "connect" | "gateway" | "config" | "agents" | "version";

export type SshConnectionStageStatus = "ok" | "failed" | "not_run" | "reused" | "interactive_required";

export type SshConnectionProbePhase = "start" | "success" | "failed" | "reused" | "interactive_required" | "completed";

export interface SshConnectionStageMetric {
  key: SshConnectionStageKey;
  latencyMs: number;
  status: SshConnectionStageStatus;
  note?: string | null;
}

export interface SshProbeProgressEvent {
  hostId: string;
  requestId: string;
  stage: SshConnectionStageKey;
  phase: SshConnectionProbePhase;
  latencyMs?: number | null;
  note?: string | null;
}

export interface SshConnectionProfile {
  probeStatus?: SshConnectionProbeStatus;
  reusedExistingConnection?: boolean;
  status: InstanceStatus;
  connectLatencyMs: number;
  gatewayLatencyMs: number;
  configLatencyMs: number;
  agentsLatencyMs?: number;
  versionLatencyMs: number;
  totalLatencyMs: number;
  quality: SshConnectionQuality;
  qualityScore: number;
  bottleneck: {
    stage: SshConnectionBottleneckStage;
    latencyMs: number;
  };
  stages?: SshConnectionStageMetric[];
}

export interface SshHost {
  id: string;
  label: string;
  host: string;
  port: number;
  username: string;
  authMethod: "key" | "ssh_config" | "password";
  keyPath?: string;
  password?: string;
  passphrase?: string;
}

export interface SshConfigHostSuggestion {
  hostAlias: string;
  hostName?: string;
  user?: string;
  port?: number;
  identityFile?: string;
}

export type SshStage =
  | "resolveHostConfig"
  | "tcpReachability"
  | "hostKeyVerification"
  | "authNegotiation"
  | "sessionOpen"
  | "remoteExec"
  | "sftpRead"
  | "sftpWrite"
  | "sftpRemove";

export type SshIntent =
  | "connect"
  | "exec"
  | "sftp_read"
  | "sftp_write"
  | "sftp_remove"
  | "install_step"
  | "doctor_remote"
  | "health_check";

export type SshDiagnosticStatus = "ok" | "degraded" | "failed";

export type SshErrorCode =
  | "SSH_HOST_UNREACHABLE"
  | "SSH_CONNECTION_REFUSED"
  | "SSH_TIMEOUT"
  | "SSH_HOST_KEY_FAILED"
  | "SSH_KEYFILE_MISSING"
  | "SSH_PASSPHRASE_REQUIRED"
  | "SSH_AUTH_FAILED"
  | "SSH_REMOTE_COMMAND_FAILED"
  | "SSH_SFTP_PERMISSION_DENIED"
  | "SSH_SESSION_STALE"
  | "SSH_UNKNOWN";

export type SshRepairAction =
  | "promptPassphrase"
  | "retryWithBackoff"
  | "switchAuthMethodToSshConfig"
  | "suggestKnownHostsBootstrap"
  | "suggestAuthorizedKeysCheck"
  | "suggestPortHostValidation"
  | "reconnectSession";

export interface SshEvidence {
  kind: string;
  value: string;
}

export interface SshDiagnosticReport {
  stage: SshStage;
  intent: SshIntent;
  status: SshDiagnosticStatus;
  errorCode?: SshErrorCode | null;
  summary: string;
  evidence: SshEvidence[];
  repairPlan: SshRepairAction[];
  confidence: number;
}

export interface SshCommandError {
  message: string;
  diagnostic: SshDiagnosticReport;
}

export interface SshExecResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

export interface SftpEntry {
  name: string;
  isDir: boolean;
  size: number;
}
