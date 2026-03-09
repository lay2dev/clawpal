import { SSH_PASSPHRASE_RETRY_HINT } from "./sshConnectErrors";
import type {
  SshConnectionProfile,
  SshConnectionProbePhase,
  SshConnectionProbeStatus,
  SshConnectionStageKey,
  SshConnectionStageMetric,
  SshProbeProgressEvent,
} from "./types";

type TranslateFn = (key: string, options?: Record<string, string | number>) => string;

export function formatSshConnectionLatency(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "-";
  if (ms >= 1000) return `${(ms / 1000).toFixed(2)} s`;
  return `${Math.round(ms)} ms`;
}

export function shouldAutoProbeSshConnectionProfile(params: {
  checked: boolean;
  checking: boolean;
  hasProfile: boolean;
  deferredInteractive: boolean;
}): boolean {
  return !params.checked && !params.checking && !params.hasProfile && !params.deferredInteractive;
}

export function shouldDeferInteractiveSshAutoProbe(rawError: string): boolean {
  return SSH_PASSPHRASE_RETRY_HINT.test(rawError);
}

export function getSshConnectionProbeStatus(
  profile: SshConnectionProfile,
): SshConnectionProbeStatus {
  return profile.probeStatus ?? "success";
}

export function shouldMarkSshProbeAsChecked(profile: SshConnectionProfile): boolean {
  return getSshConnectionProbeStatus(profile) !== "interactive_required";
}

export function getSshConnectionStageMetrics(
  profile: SshConnectionProfile,
): SshConnectionStageMetric[] {
  if (profile.stages && profile.stages.length > 0) return profile.stages;
  return [
    { key: "connect", latencyMs: profile.connectLatencyMs, status: "ok" },
    { key: "gateway", latencyMs: profile.gatewayLatencyMs, status: "ok" },
    { key: "config", latencyMs: profile.configLatencyMs, status: "ok" },
    { key: "agents", latencyMs: profile.agentsLatencyMs ?? 0, status: "ok" },
    { key: "version", latencyMs: profile.versionLatencyMs, status: "ok" },
  ];
}

export function getSshConnectionFailureSummary(profile: SshConnectionProfile): string | null {
  return profile.status.sshDiagnostic?.summary ?? null;
}

function inferStageStatusFromPhase(phase: SshConnectionProbePhase): SshConnectionStageMetric["status"] | null {
  switch (phase) {
    case "success":
      return "ok";
    case "failed":
      return "failed";
    case "reused":
      return "reused";
    case "interactive_required":
      return "interactive_required";
    default:
      return null;
  }
}

export function buildSshProbeProgressLine(
  progress: SshProbeProgressEvent,
  t: TranslateFn,
): string {
  const stageLabel = t(`start.sshStage.${progress.stage}`);
  switch (progress.phase) {
    case "start":
      return t("start.sshProbe.start", { stage: stageLabel });
    case "success":
      return t("start.sshProbe.success", { stage: stageLabel });
    case "failed":
      return t("start.sshProbe.failed", { stage: stageLabel });
    case "reused":
      return t("start.sshProbe.reused", { stage: stageLabel });
    case "interactive_required":
      return t("start.sshProbe.interactiveRequired", { stage: stageLabel });
    case "completed":
      return t("start.sshProbe.completed", { stage: stageLabel });
    default:
      return t("start.checking");
  }
}

export function mergeSshProbeStageMetric(
  current: SshConnectionStageMetric[],
  progress: SshProbeProgressEvent,
): SshConnectionStageMetric[] {
  const nextStatus = inferStageStatusFromPhase(progress.phase);
  if (!nextStatus) return current;
  const nextMetric: SshConnectionStageMetric = {
    key: progress.stage,
    latencyMs: progress.latencyMs ?? 0,
    status: nextStatus,
    note: progress.note ?? null,
  };
  const withoutStage = current.filter((entry) => entry.key !== progress.stage);
  return [...withoutStage, nextMetric];
}

function orderedStageMetrics(
  recorded: SshConnectionStageMetric[],
  failingStage: SshConnectionStageKey,
  errorSummary: string,
): SshConnectionStageMetric[] {
  const stageOrder: SshConnectionStageKey[] = ["connect", "gateway", "config", "agents", "version"];
  const byKey = new Map(recorded.map((entry) => [entry.key, entry]));
  const failureIndex = stageOrder.indexOf(failingStage);
  return stageOrder.map((key, index) => {
    const existing = byKey.get(key);
    if (existing) {
      if (key === failingStage && existing.status !== "failed") {
        return { ...existing, status: "failed", note: existing.note ?? errorSummary };
      }
      return existing;
    }
    if (index < failureIndex) {
      return { key, latencyMs: 0, status: "ok" };
    }
    if (key === failingStage) {
      return { key, latencyMs: 0, status: "failed", note: errorSummary };
    }
    return { key, latencyMs: 0, status: "not_run" };
  });
}

export function buildFailedSshConnectionProfileFromProgress(params: {
  errorSummary: string;
  progressStages: SshConnectionStageMetric[];
  failingStage?: SshConnectionStageKey;
}): SshConnectionProfile {
  const failingStage = params.failingStage ?? "connect";
  const stages = orderedStageMetrics(params.progressStages, failingStage, params.errorSummary);
  const latencyByStage = new Map(stages.map((stage) => [stage.key, stage.latencyMs]));
  const bottleneck = stages.reduce<{ stage: SshConnectionStageKey | "other"; latencyMs: number }>(
    (current, stage) => stage.latencyMs > current.latencyMs ? { stage: stage.key, latencyMs: stage.latencyMs } : current,
    { stage: failingStage, latencyMs: latencyByStage.get(failingStage) ?? 0 },
  );

  return {
    probeStatus: "failed",
    reusedExistingConnection: false,
    status: {
      healthy: false,
      activeAgents: 0,
      sshDiagnostic: {
        stage: failingStage === "config" ? "sftpRead" : failingStage === "connect" ? "sessionOpen" : "remoteExec",
        intent: "health_check",
        status: "failed",
        errorCode: "SSH_TIMEOUT",
        summary: params.errorSummary,
        evidence: [],
        repairPlan: [],
        confidence: 0.6,
      },
    },
    connectLatencyMs: latencyByStage.get("connect") ?? 0,
    gatewayLatencyMs: latencyByStage.get("gateway") ?? 0,
    configLatencyMs: latencyByStage.get("config") ?? 0,
    agentsLatencyMs: latencyByStage.get("agents") ?? 0,
    versionLatencyMs: latencyByStage.get("version") ?? 0,
    totalLatencyMs: stages.reduce((sum, stage) => sum + stage.latencyMs, 0),
    quality: "unknown",
    qualityScore: 0,
    bottleneck,
    stages,
  };
}
