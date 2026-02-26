import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type {
  EnsureAccessResult,
  ModelProfile,
  InstallMethod,
  InstallMethodCapability,
  InstallTargetDecision,
  InstallUiAction,
  InstallSession,
  InstallStep,
  InstallStepResult,
  SshHost,
} from "@/lib/types";
import { useApi } from "@/lib/use-api";
import { appendOrchestratorEvent } from "@/lib/orchestrator-log";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";

const METHOD_ORDER: InstallMethod[] = ["local", "wsl2", "docker", "remote_ssh"];
const INSTALL_SESSION_STORAGE_PREFIX = "clawpal_install_session_v1:";
const INSTALL_RESUME_STORAGE_PREFIX = "clawpal_install_resume_v1:";
const DOCKER_INSTANCES_STORAGE_KEY = "clawpal_docker_instances";
const DEFAULT_DOCKER_INSTANCE_ID = "docker:local";
const INTENT_HINTS = ["本机", "Docker", "远程 SSH", "连接已有实例"];

type BlockerAction = "resume" | "settings" | "doctor" | "instances";

interface InstallAutoBlocker {
  code: string;
  message: string;
  details?: string;
  actions: BlockerAction[];
}

type StepState = "done" | "running" | "failed" | "pending";

function getStepState(session: InstallSession, step: InstallStep): StepState {
  const stepOrder: InstallStep[] = ["precheck", "install", "init", "verify"];
  const currentIdx = session.current_step ? stepOrder.indexOf(session.current_step) : -1;
  const thisIdx = stepOrder.indexOf(step);

  if (session.state === "ready") return "done";
  if (session.current_step === step) {
    const stateStr = session.state;
    if (stateStr.endsWith("_failed")) return "failed";
    if (stateStr.endsWith("_running")) return "running";
    return "running";
  }
  if (thisIdx < currentIdx) return "done";
  return "pending";
}

function StepIndicator({ state }: { state: StepState }) {
  if (state === "done") return <span className="size-4 rounded-full bg-green-500 flex items-center justify-center text-white text-[10px]">✓</span>;
  if (state === "running") return <span className="size-4 rounded-full border-2 border-primary border-t-transparent animate-spin" />;
  if (state === "failed") return <span className="size-4 rounded-full bg-red-500 flex items-center justify-center text-white text-[10px]">✕</span>;
  return <span className="size-4 rounded-full border-2 border-muted-foreground/30" />;
}

function hasStorage(): boolean {
  return typeof window !== "undefined" && typeof window.localStorage !== "undefined";
}

function storageSessionKey(instanceId: string): string {
  return `${INSTALL_SESSION_STORAGE_PREFIX}${instanceId || "local"}`;
}

function storageResumeKey(instanceId: string): string {
  return `${INSTALL_RESUME_STORAGE_PREFIX}${instanceId || "local"}`;
}

function readStoredSessionId(instanceId: string): string | null {
  if (!hasStorage()) return null;
  return window.localStorage.getItem(storageSessionKey(instanceId));
}

function writeStoredSessionId(instanceId: string, sessionId: string): void {
  if (!hasStorage()) return;
  window.localStorage.setItem(storageSessionKey(instanceId), sessionId);
}

function clearStoredSessionId(instanceId: string): void {
  if (!hasStorage()) return;
  window.localStorage.removeItem(storageSessionKey(instanceId));
}

function readResumeSessionId(instanceId: string): string | null {
  if (!hasStorage()) return null;
  return window.localStorage.getItem(storageResumeKey(instanceId));
}

function writeResumeSessionId(instanceId: string, sessionId: string): void {
  if (!hasStorage()) return;
  window.localStorage.setItem(storageResumeKey(instanceId), sessionId);
}

function clearResumeSessionId(instanceId: string): void {
  if (!hasStorage()) return;
  window.localStorage.removeItem(storageResumeKey(instanceId));
}

function readStoredDockerInstanceIds(): Set<string> {
  if (!hasStorage()) return new Set();
  const raw = window.localStorage.getItem(DOCKER_INSTANCES_STORAGE_KEY);
  if (!raw) return new Set();
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(
      parsed
        .map((item) => (typeof item?.id === "string" ? item.id.trim() : ""))
        .filter((id) => id.length > 0),
    );
  } catch {
    return new Set();
  }
}

function allocateDockerInstanceMeta(): { id: string; label: string } {
  const ids = readStoredDockerInstanceIds();
  if (!ids.has(DEFAULT_DOCKER_INSTANCE_ID)) {
    return { id: DEFAULT_DOCKER_INSTANCE_ID, label: "Docker Local" };
  }
  let index = 2;
  while (ids.has(`docker:local-${index}`)) {
    index += 1;
  }
  return { id: `docker:local-${index}`, label: `Docker Local ${index}` };
}

function classifyAutoBlocker(
  error: string,
  fallbackMessage: string,
  errorCode?: string | null,
  actionHint?: string | null,
): InstallAutoBlocker {
  if (actionHint === "open_settings_auth") {
    return {
      code: errorCode || "auth_missing",
      message: fallbackMessage,
      details: error,
      actions: ["settings", "resume"],
    };
  }
  if (actionHint === "open_instances") {
    return {
      code: errorCode || "remote_target_missing",
      message: fallbackMessage,
      details: error,
      actions: ["instances", "resume"],
    };
  }
  if (actionHint === "open_doctor") {
    return {
      code: errorCode || "diagnosis_required",
      message: fallbackMessage,
      details: error,
      actions: ["doctor", "resume"],
    };
  }
  if (errorCode === "permission_denied") {
    return {
      code: "permission_denied",
      message: fallbackMessage,
      details: error,
      actions: ["doctor", "resume"],
    };
  }
  if (errorCode === "network_error") {
    return {
      code: "network_error",
      message: fallbackMessage,
      details: error,
      actions: ["doctor", "resume"],
    };
  }
  if (errorCode === "env_missing") {
    return {
      code: "env_missing",
      message: fallbackMessage,
      details: error,
      actions: ["doctor", "resume"],
    };
  }
  const lower = error.toLowerCase();
  if (
    lower.includes("no compatible api key found")
    || lower.includes("no auth profile")
    || lower.includes("openrouter_api_key")
    || lower.includes("anthropic_api_key")
    || lower.includes("openai_api_key")
  ) {
    return {
      code: "auth_missing",
      message: fallbackMessage,
      details: error,
      actions: ["settings", "resume"],
    };
  }
  if (
    lower.includes("no ssh host config with id")
    || lower.includes("remote ssh host not found")
    || lower.includes("remote ssh target missing")
  ) {
    return {
      code: "remote_target_missing",
      message: fallbackMessage,
      details: error,
      actions: ["instances", "resume"],
    };
  }
  if (
    lower.includes("cannot connect to the docker daemon")
    || lower.includes("docker: command not found")
    || lower.includes("command failed: docker")
  ) {
    return {
      code: "docker_unavailable",
      message: fallbackMessage,
      details: error,
      actions: ["doctor", "resume"],
    };
  }
  if (lower.includes("permission denied") || lower.includes("operation not permitted")) {
    return {
      code: "permission_denied",
      message: fallbackMessage,
      details: error,
      actions: ["doctor", "resume"],
    };
  }
  if (lower.includes("network") || lower.includes("timed out") || lower.includes("failed to connect")) {
    return {
      code: "network_error",
      message: fallbackMessage,
      details: error,
      actions: ["doctor", "resume"],
    };
  }
  return {
    code: "unknown",
    message: fallbackMessage,
    details: error,
    actions: ["resume"],
  };
}

function sortMethods(methods: InstallMethodCapability[]): InstallMethodCapability[] {
  const rank = new Map(METHOD_ORDER.map((method, index) => [method, index]));
  return [...methods].sort((a, b) => (rank.get(a.method) ?? 99) - (rank.get(b.method) ?? 99));
}

export function InstallHub({
  open,
  onOpenChange,
  showToast,
  onNavigate,
  onReady,
  onRequestAddSsh,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  showToast?: (message: string, type?: "success" | "error") => void;
  onNavigate?: (route: string) => void;
  onReady?: (session: InstallSession) => void;
  onRequestAddSsh?: () => void;
}) {
  const { t } = useTranslation();
  const ua = useApi();
  const [methods, setMethods] = useState<InstallMethodCapability[]>([]);
  const [loadingMethods, setLoadingMethods] = useState(true);
  const [selectedMethod, setSelectedMethod] = useState<InstallMethod>("local");
  const [creating, setCreating] = useState(false);
  const [runningStep, setRunningStep] = useState<InstallStep | null>(null);
  const [autoRunning, setAutoRunning] = useState(false);
  const [session, setSession] = useState<InstallSession | null>(null);
  const [lastResult, setLastResult] = useState<InstallStepResult | null>(null);
  const [lastAccessResult, setLastAccessResult] = useState<EnsureAccessResult | null>(null);
  const [lastAccessError, setLastAccessError] = useState<string | null>(null);
  const [ensuringAccess, setEnsuringAccess] = useState(false);
  const [lastOrchestratorReason, setLastOrchestratorReason] = useState<string>("");
  const [lastOrchestratorSource, setLastOrchestratorSource] = useState<string>("");
  const [autoBlocker, setAutoBlocker] = useState<InstallAutoBlocker | null>(null);
  const [resumeSessionId, setResumeSessionId] = useState<string | null>(null);
  const [sshHosts, setSshHosts] = useState<SshHost[]>([]);
  const [selectedSshHostId, setSelectedSshHostId] = useState<string>("");
  const [installIntent, setInstallIntent] = useState("本机 Docker");
  const [checkingAuth, setCheckingAuth] = useState(false);
  const [syncingAuth, setSyncingAuth] = useState(false);
  const [authRequired, setAuthRequired] = useState(false);
  const [decidingTarget, setDecidingTarget] = useState(false);
  const [targetDecision, setTargetDecision] = useState<InstallTargetDecision | null>(null);

  useEffect(() => {
    setLoadingMethods(true);
    ua.listInstallMethods()
      .then((result) => {
        const sorted = sortMethods(result);
        setMethods(sorted);
        if (sorted.length > 0) {
          setSelectedMethod(sorted[0].method);
        }
      })
      .catch((e) => showToast?.(String(e), "error"))
      .finally(() => setLoadingMethods(false));
  }, [ua, showToast]);

  useEffect(() => {
    ua.listSshHosts()
      .then((hosts) => {
        setSshHosts(hosts);
        setSelectedSshHostId((prev) => (
          prev && hosts.some((host) => host.id === prev) ? prev : ""
        ));
      })
      .catch(() => {});
  }, [ua]);

  useEffect(() => {
    const instanceId = ua.instanceId || "local";
    setSession(null);
    setAutoBlocker(null);
    setLastResult(null);
    setLastAccessResult(null);
    setLastAccessError(null);
    setLastOrchestratorReason("");
    setLastOrchestratorSource("");
    setTargetDecision(null);
    setResumeSessionId(readResumeSessionId(instanceId));

    const sessionId = readStoredSessionId(instanceId);
    if (!sessionId) return;
    ua.installGetSession(sessionId)
      .then((restored) => {
        setSession(restored);
        setSelectedMethod(restored.method);
        if (restored.state === "ready") {
          clearResumeSessionId(instanceId);
          setResumeSessionId(null);
        }
      })
      .catch(() => {
        clearStoredSessionId(instanceId);
        clearResumeSessionId(instanceId);
        setResumeSessionId(null);
      });
  }, [ua.instanceId]);

  const selectedTargetMeta = useMemo(
    () => methods.find((m) => m.method === targetDecision?.method) ?? null,
    [methods, targetDecision?.method],
  );
  const targetUiActions = targetDecision?.uiActions ?? [];
  const targetRequiredFields = targetDecision?.requiredFields ?? [];
  const targetRequiresSshHost = Boolean(
    targetDecision?.requiresSshHost || targetRequiredFields.includes("ssh_host_id"),
  );
  const intentPlaceholder = t("home.install.intentPlaceholder");
  const methodLabel = (method: InstallMethod): string => t(`home.install.method.${method}`);
  const requiredFieldLabel = (field: string): string => {
    if (field === "ssh_host_id") return t("home.install.selectRemoteHost");
    if (field === "auth_profile") return t("home.install.authRequiredTitle");
    return field;
  };
  const resolveGoal = (targetSession: InstallSession): string => {
    const fromArtifacts = typeof targetSession.artifacts?.install_goal_intent === "string"
      ? targetSession.artifacts.install_goal_intent.trim()
      : "";
    return fromArtifacts || `install:${targetSession.method}`;
  };

  const checkCompatibleAuth = async (): Promise<boolean> => {
    setCheckingAuth(true);
    try {
      const keys = await api.resolveApiKeys();
      const hasAny = keys.some((item) => Boolean(item.maskedKey && item.maskedKey.trim().length > 0));
      setAuthRequired(!hasAny);
      return hasAny;
    } catch (e) {
      showToast?.(String(e), "error");
      setAuthRequired(true);
      return false;
    } finally {
      setCheckingAuth(false);
    }
  };

  const syncAuthFromRemoteHost = async (): Promise<boolean> => {
    if (!selectedSshHostId) {
      showToast?.(t("home.install.remoteHostRequired"), "error");
      return false;
    }
    setSyncingAuth(true);
    try {
      await api.sshConnect(selectedSshHostId).catch(() => {});
      const profiles = await api.remoteListModelProfiles(selectedSshHostId);
      let imported = 0;
      for (const profile of profiles as ModelProfile[]) {
        await api.upsertModelProfile(profile);
        imported += 1;
      }
      showToast?.(t("home.install.authSynced", { count: imported }), "success");
      showToast?.(t("home.install.authRotateHint"), "error");
      setAuthRequired(false);
      return true;
    } catch (e) {
      showToast?.(t("home.install.authSyncFailed", { error: String(e) }), "error");
      return false;
    } finally {
      setSyncingAuth(false);
    }
  };

  const ensureInstanceByMethod = (nextSession: InstallSession): { instanceId: string; transport: string } | null => {
    if (nextSession.method === "local") {
      return { instanceId: "local", transport: "local" };
    }
    if (nextSession.method === "docker") {
      const instanceId = typeof nextSession.artifacts?.docker_instance_id === "string"
        ? nextSession.artifacts.docker_instance_id.trim()
        : "";
      return { instanceId: instanceId || DEFAULT_DOCKER_INSTANCE_ID, transport: "docker_local" };
    }
    if (nextSession.method === "wsl2") {
      return { instanceId: "wsl2:local", transport: "wsl2" };
    }
    if (nextSession.method === "remote_ssh") {
      const hostId = (nextSession.artifacts?.ssh_host_id as string | undefined) || selectedSshHostId;
      if (!hostId) return null;
      return { instanceId: hostId, transport: "remote_ssh" };
    }
    return null;
  };

  const runEnsureAccess = async (nextSession: InstallSession): Promise<EnsureAccessResult | null> => {
    const target = ensureInstanceByMethod(nextSession);
    if (!target) return null;
    setEnsuringAccess(true);
    setLastAccessError(null);
    try {
      const result = await ua.ensureAccessProfile(target.instanceId, target.transport);
      setLastAccessResult(result);
      showToast?.(
        t("home.install.access.ready", {
          chain: result.workingChain.join(" -> "),
        }),
        "success",
      );
      appendOrchestratorEvent({
        level: "success",
        message: "access discovery completed",
        instanceId: target.instanceId,
        sessionId: nextSession.id,
        goal: `install:${nextSession.method}`,
        source: "aad",
        state: nextSession.state,
        details: result.workingChain.join(" -> "),
      });
      return result;
    } catch (e) {
      const message = String(e);
      setLastAccessError(message);
      showToast?.(t("home.install.access.failed", { error: message }), "error");
      appendOrchestratorEvent({
        level: "error",
        message: "access discovery failed",
        instanceId: target.instanceId,
        sessionId: nextSession.id,
        goal: `install:${nextSession.method}`,
        source: "aad",
        state: nextSession.state,
        details: message,
      });
      return null;
    } finally {
      setEnsuringAccess(false);
    }
  };

  const runRecordExperience = async (nextSession: InstallSession) => {
    const target = ensureInstanceByMethod(nextSession);
    if (!target) return;
    try {
      const result = await ua.recordInstallExperience(
        nextSession.id,
        target.instanceId,
        `install:${nextSession.method}`,
      );
      showToast?.(
        t("home.install.access.experienceSaved", { count: result.totalCount }),
        "success",
      );
      appendOrchestratorEvent({
        level: "success",
        message: "experience saved",
        instanceId: target.instanceId,
        sessionId: nextSession.id,
        goal: `install:${nextSession.method}`,
        source: "experience-store",
        state: nextSession.state,
        details: `total=${result.totalCount}`,
      });
    } catch (e) {
      showToast?.(t("home.install.access.experienceFailed", { error: String(e) }), "error");
      appendOrchestratorEvent({
        level: "error",
        message: "experience save failed",
        instanceId: target.instanceId,
        sessionId: nextSession.id,
        goal: `install:${nextSession.method}`,
        source: "experience-store",
        state: nextSession.state,
        details: String(e),
      });
    }
  };

  const runStepAndRefresh = async (
    targetSession: InstallSession,
    step: InstallStep,
    quiet = false,
  ): Promise<{ result: InstallStepResult; session: InstallSession | null }> => {
    setRunningStep(step);
    try {
      const result = await ua.installRunStep(targetSession.id, step);
      setLastResult(result);
      if (!quiet) {
        showToast?.(result.summary, result.ok ? "success" : "error");
      }
      const next = await refreshSession(targetSession.id);
      if (result.ok) {
        setAutoBlocker(null);
      }
      const target = ensureInstanceByMethod(next);
      appendOrchestratorEvent({
        level: result.ok ? "success" : "error",
        message: result.summary,
        instanceId: target?.instanceId || "local",
        sessionId: targetSession.id,
        goal: `install:${targetSession.method}`,
        source: "step-runner",
        step,
        state: next.state,
        details: result.details,
      });
      if (next.state === "init_passed" || next.state === "ready") {
        await runEnsureAccess(next);
      }
      if (next.state === "ready") {
        await runRecordExperience(next);
        onReady?.(next);
      }
      return { result, session: next };
    } catch (e) {
      const message = String(e);
      if (!quiet) {
        showToast?.(message, "error");
      }
      return {
        result: {
          ok: false,
          summary: message,
          details: message,
          commands: [],
          artifacts: {},
          next_step: null,
          error_code: "runtime_error",
        },
        session: null,
      };
    } finally {
      setRunningStep(null);
    }
  };

  const runAutoInstall = async (startSession: InstallSession) => {
    setAutoRunning(true);
    setAutoBlocker(null);
    try {
      let current = startSession;
      const goal = resolveGoal(startSession);
      const initialTarget = ensureInstanceByMethod(startSession);
      appendOrchestratorEvent({
        level: "info",
        message: "auto-install started",
        instanceId: initialTarget?.instanceId || "local",
        sessionId: startSession.id,
        goal,
        source: "orchestrator",
        state: startSession.state,
      });
      while (current.state !== "ready") {
        let step: InstallStep | null = null;
        try {
          const decision = await ua.installOrchestratorNext(current.id, goal);
          setLastOrchestratorReason(decision.reason || "");
          setLastOrchestratorSource(decision.source || "");
          if (decision.source !== "zeroclaw-sidecar") {
            const blocker = classifyAutoBlocker(
              decision.reason || "",
              t("home.install.blocked.orchestratorSource", { source: decision.source }),
              decision.errorCode,
              decision.actionHint,
            );
            setAutoBlocker(blocker);
            const target = ensureInstanceByMethod(current);
            appendOrchestratorEvent({
              level: "error",
              message: "orchestrator fallback blocked (strict mode)",
              instanceId: target?.instanceId || "local",
              sessionId: current.id,
              goal,
              source: decision.source,
              state: current.state,
              details: decision.reason,
            });
            showToast?.(t("home.install.orchestratorStrict", { source: decision.source }), "error");
            return;
          }
          step = decision.step as InstallStep | null;
          const target = ensureInstanceByMethod(current);
          appendOrchestratorEvent({
            level: "info",
            message: `orchestrator selected step: ${decision.step || "stop"}`,
            instanceId: target?.instanceId || "local",
            sessionId: current.id,
            goal,
            source: decision.source,
            state: current.state,
            details: decision.reason,
          });
        } catch (e) {
          const blocker = classifyAutoBlocker(
            String(e),
            t("home.install.blocked.orchestratorUnavailable"),
          );
          setAutoBlocker(blocker);
          setLastOrchestratorReason(String(e));
          setLastOrchestratorSource("error");
          const target = ensureInstanceByMethod(current);
          appendOrchestratorEvent({
            level: "error",
            message: "orchestrator decision failed",
            instanceId: target?.instanceId || "local",
            sessionId: current.id,
            goal,
            source: "error",
            state: current.state,
            details: String(e),
          });
          showToast?.(t("home.install.orchestratorUnavailable", { error: String(e) }), "error");
          return;
        }
        if (!step) {
          const blocker = classifyAutoBlocker(
            "orchestrator returned no step",
            t("home.install.blocked.orchestratorUnavailable"),
          );
          setAutoBlocker(blocker);
          showToast?.(t("home.install.blocked.orchestratorUnavailable"), "error");
          return;
        }
        const { result, session: refreshed } = await runStepAndRefresh(current, step, true);
        if (!result.ok || !refreshed) {
          const blocker = classifyAutoBlocker(
            result.details || result.summary,
            t("home.install.blocked.stepFailed", { step: t(`home.install.step.${step}`) }),
            result.error_code,
          );
          setAutoBlocker(blocker);
          showToast?.(result.summary, "error");
          return;
        }
        current = refreshed;
      }
      if (current.state === "ready") {
        setAutoBlocker(null);
        showToast?.(t("home.install.autoDone"), "success");
        const target = ensureInstanceByMethod(current);
        appendOrchestratorEvent({
          level: "success",
          message: "auto-install completed",
          instanceId: target?.instanceId || "local",
          sessionId: current.id,
          goal,
          source: "orchestrator",
          state: current.state,
        });
      }
    } finally {
      setAutoRunning(false);
    }
  };

  useEffect(() => {
    const instanceId = ua.instanceId || "local";
    if (!session) {
      clearStoredSessionId(instanceId);
      return;
    }
    writeStoredSessionId(instanceId, session.id);
    if (session.state === "ready") {
      clearResumeSessionId(instanceId);
      if (resumeSessionId) {
        setResumeSessionId(null);
      }
    }
  }, [session, ua.instanceId, resumeSessionId]);

  useEffect(() => {
    if (!session || !resumeSessionId) return;
    if (session.id !== resumeSessionId) return;
    if (session.state === "ready") {
      clearResumeSessionId(ua.instanceId || "local");
      setResumeSessionId(null);
      return;
    }
    if (autoRunning || creating || runningStep !== null) return;
    clearResumeSessionId(ua.instanceId || "local");
    setResumeSessionId(null);
    void runAutoInstall(session);
  }, [session, resumeSessionId, autoRunning, creating, runningStep, ua.instanceId]);

  const navigateWithAutoResume = (route: string, keepResumeMarker = false) => {
    if (keepResumeMarker && session) {
      const instanceId = ua.instanceId || "local";
      writeResumeSessionId(instanceId, session.id);
      setResumeSessionId(session.id);
    }
    onNavigate?.(route);
  };

  const createInstallSession = async (method: InstallMethod, goalIntent: string) => {
    setCreating(true);
    setLastResult(null);
    setLastAccessResult(null);
    setLastAccessError(null);
    setAutoBlocker(null);
    setSelectedMethod(method);
    const dockerMeta = method === "docker" ? allocateDockerInstanceMeta() : null;
    const options = method === "remote_ssh"
      ? {
          ssh_host_id: selectedSshHostId,
          install_goal_intent: goalIntent,
        }
      : method === "docker"
        ? {
            docker_instance_id: dockerMeta?.id || DEFAULT_DOCKER_INSTANCE_ID,
            docker_instance_label: dockerMeta?.label || "Docker Local",
            install_goal_intent: goalIntent,
          }
        : {
            install_goal_intent: goalIntent,
          };
    ua.installCreateSession(method, options)
      .then((next) => {
        setSession(next);
        showToast?.(t("home.install.sessionCreated"), "success");
        const target = ensureInstanceByMethod(next);
        appendOrchestratorEvent({
          level: "info",
          message: "install session created",
          instanceId: target?.instanceId || "local",
          sessionId: next.id,
          goal: `install:${next.method}`,
          source: "ui",
          state: next.state,
        });
        void runAutoInstall(next);
      })
      .catch((e) => showToast?.(String(e), "error"))
      .finally(() => setCreating(false));
  };

  const handleLauncherConfirm = async () => {
    const goalIntent = installIntent.trim() || "install";
    const intentLower = goalIntent.toLowerCase();
    const isConnect = intentLower.includes("连接") || intentLower.includes("connect");
    const mode = isConnect ? "connect" : "install";

    setDecidingTarget(true);
    let decision: InstallTargetDecision;
    try {
      decision = await ua.installDecideTarget(goalIntent, {
        selected_ssh_host_id: selectedSshHostId || null,
        ssh_host_count: sshHosts.length,
        available_methods: methods
          .filter((item) => item.available)
          .map((item) => item.method),
        mode,
      });
    } catch (e) {
      const message = String(e);
      showToast?.(message, "error");
      setDecidingTarget(false);
      return;
    } finally {
      setDecidingTarget(false);
    }

    setTargetDecision(decision);

    if (decision.source !== "zeroclaw-sidecar" || !decision.method) {
      showToast?.(decision.reason || t("home.install.targetDecisionFailed"), "error");
      return;
    }

    const decisionMethod = decision.method;
    const decisionNeedsSshHost = Boolean(
      decision.requiresSshHost || decision.requiredFields?.includes("ssh_host_id"),
    );
    if (decisionNeedsSshHost && !selectedSshHostId) {
      showToast?.(t("home.install.remoteHostRequired"), "error");
      return;
    }

    if (isConnect) {
      // Connect flow: sync auth from remote if SSH, then signal done
      if (decisionMethod === "remote_ssh" && selectedSshHostId) {
        await syncAuthFromRemoteHost().catch(() => false);
      }
      showToast?.(t("home.install.connectDone"), "success");
      return;
    }

    // Install flow
    const methodMeta = methods.find((item) => item.method === decisionMethod) ?? null;
    if (!methodMeta?.available) {
      showToast?.(methodMeta?.hint || t("home.install.needsSetup"), "error");
      return;
    }

    const hasAuth = await checkCompatibleAuth();
    if (!hasAuth) {
      setAuthRequired(true);
      return;
    }

    setAuthRequired(false);
    await createInstallSession(decisionMethod, goalIntent);
  };

  const refreshSession = (sessionId: string) => {
    return ua.installGetSession(sessionId).then((next) => {
      setSession(next);
      return next;
    });
  };

  const runTargetUiAction = (action: InstallUiAction) => {
    if (action.kind === "open_settings") {
      onNavigate?.("settings");
      return;
    }
    if (action.kind === "open_instances") {
      if (onRequestAddSsh) {
        onRequestAddSsh();
      } else {
        onNavigate?.("home");
      }
      return;
    }
    if (action.kind === "open_doctor") {
      onNavigate?.("doctor");
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{t("home.install.setupTitle")}</DialogTitle>
          <DialogDescription>{t("home.install.setupDesc")}</DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          {autoRunning && <Badge variant="outline">{t("home.install.autoRunning")}</Badge>}

          <div className="flex flex-wrap items-center gap-2 text-xs min-h-5">
            {targetDecision?.method && (
              <Badge variant="outline">{methodLabel(targetDecision.method)}</Badge>
            )}
            {targetDecision?.method && selectedTargetMeta && (
              <Badge variant={selectedTargetMeta.available ? "secondary" : "outline"}>
                {selectedTargetMeta.available
                  ? t("home.install.available")
                  : t("home.install.needsSetup")}
              </Badge>
            )}
          </div>
          {targetDecision?.reason && targetDecision.source !== "zeroclaw-sidecar" && (
            <p className="text-xs text-muted-foreground">{targetDecision.reason}</p>
          )}
          {targetRequiredFields.length > 0 && (
            <div className="flex flex-wrap items-center gap-1.5 text-xs">
              {targetRequiredFields.map((field) => (
                <Badge key={field} variant="outline">
                  {requiredFieldLabel(field)}
                </Badge>
              ))}
            </div>
          )}
          {targetUiActions.length > 0 && (
            <div className="flex flex-wrap items-center gap-2">
              {targetUiActions.map((action) => (
                <Button
                  key={action.id}
                  size="xs"
                  variant="outline"
                  onClick={() => runTargetUiAction(action)}
                >
                  {action.label}
                </Button>
              ))}
            </div>
          )}

          {(authRequired
            || decidingTarget
            || targetRequiresSshHost
            || sshHosts.length > 0) && (
            <div className="space-y-2">
              <div className="text-xs font-medium">{t("home.install.selectRemoteHost")}</div>
              <Select
                value={selectedSshHostId}
                onValueChange={setSelectedSshHostId}
                disabled={creating || runningStep !== null || autoRunning || sshHosts.length === 0}
              >
                <SelectTrigger size="sm">
                  <SelectValue placeholder={t("home.install.selectRemoteHost")} />
                </SelectTrigger>
                <SelectContent>
                  {sshHosts.map((host) => (
                    <SelectItem key={host.id} value={host.id}>
                      {host.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {sshHosts.length === 0 && (
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">{t("home.install.noRemoteHosts")}</span>
                  <Button
                    size="xs"
                    variant="outline"
                    onClick={() => onRequestAddSsh?.()}
                  >
                    {t("instance.addSsh")}
                  </Button>
                </div>
              )}
            </div>
          )}

          {authRequired && (
            <div className="rounded border border-amber-500/40 bg-amber-500/5 p-2 text-xs space-y-2">
              <div className="font-medium">{t("home.install.authRequiredHint")}</div>
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  size="xs"
                  variant="outline"
                  onClick={() => onNavigate?.("settings")}
                >
                  {t("home.install.goSettings")}
                </Button>
                <Button
                  size="xs"
                  variant="outline"
                  disabled={syncingAuth || !selectedSshHostId}
                  onClick={() => void syncAuthFromRemoteHost()}
                >
                  {syncingAuth
                    ? t("home.install.syncingAuth")
                    : t("home.install.syncAuthFromRemote")}
                </Button>
              </div>
            </div>
          )}

          {/* Intent input */}
          <div className="space-y-2">
            <Textarea
              value={installIntent}
              onChange={(event) => setInstallIntent(event.target.value)}
              placeholder={intentPlaceholder}
              className="min-h-16"
            />
            <div className="flex flex-wrap items-center gap-1.5">
              {INTENT_HINTS.map((hint) => (
                <button
                  key={hint}
                  type="button"
                  className="text-xs px-2 py-1 rounded border hover:bg-muted/40 transition-colors"
                  onClick={() => setInstallIntent(hint)}
                >
                  {hint}
                </button>
              ))}
            </div>
            <div className="flex items-center justify-end">
              <Button
                onClick={() => void handleLauncherConfirm()}
                disabled={
                  loadingMethods
                  || creating
                  || decidingTarget
                  || checkingAuth
                  || syncingAuth
                  || (targetRequiresSshHost && !selectedSshHostId)
                }
              >
                {t("home.install.launcherRun")}
              </Button>
            </div>
          </div>

          {/* A2UI Stepper */}
          {session && (
            <div className="space-y-2 mt-4">
              {(["precheck", "install", "init", "verify"] as const).map((step) => {
                const state = getStepState(session, step);
                return (
                  <div key={step} className="flex items-center gap-2.5">
                    <StepIndicator state={state} />
                    <span className={cn(
                      "text-sm",
                      state === "running" && "font-medium text-primary",
                      state === "failed" && "text-destructive",
                      state === "done" && "text-foreground",
                      state === "pending" && "text-muted-foreground",
                    )}>
                      {t(`home.install.step.${step}`)}
                    </span>
                    {state === "running" && (
                      <span className="text-xs text-muted-foreground animate-pulse">
                        {t("home.install.running")}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          )}

          {/* Blocker recovery */}
          {autoBlocker && session && session.state !== "ready" && !autoRunning && (
            <Card className="border-destructive/30 bg-destructive/5 mt-4">
              <CardContent className="space-y-3">
                <p className="text-sm font-medium">{autoBlocker.message}</p>
                {autoBlocker.details && (
                  <details className="text-xs text-muted-foreground">
                    <summary className="cursor-pointer">{t("home.install.showDetails")}</summary>
                    <pre className="mt-1 whitespace-pre-wrap font-mono">{autoBlocker.details}</pre>
                  </details>
                )}
                <div className="flex gap-2">
                  {autoBlocker.actions.includes("settings") && (
                    <Button size="sm" variant="outline" onClick={() => onNavigate?.("settings")}>
                      {t("home.install.goSettings")}
                    </Button>
                  )}
                  {autoBlocker.actions.includes("doctor") && (
                    <Button size="sm" variant="outline" onClick={() => onNavigate?.("doctor")}>
                      {t("home.install.openDoctor")}
                    </Button>
                  )}
                  {autoBlocker.actions.includes("resume") && (
                    <Button size="sm" onClick={() => { setAutoBlocker(null); void runAutoInstall(session); }}>
                      {t("home.install.retry")}
                    </Button>
                  )}
                </div>
              </CardContent>
            </Card>
          )}

          {/* Completion */}
          {session?.state === "ready" && (
            <div className="space-y-3 mt-4">
              <p className="text-sm text-green-600 dark:text-green-400 font-medium">
                {t("home.install.ready")}
              </p>
              <div className="flex gap-2">
                <Button size="sm" onClick={() => { onOpenChange(false); onNavigate?.("settings"); }}>
                  {t("home.install.goSettings")}
                </Button>
                <Button size="sm" variant="outline" onClick={() => { onOpenChange(false); onNavigate?.("channels"); }}>
                  {t("home.install.goChannels")}
                </Button>
              </div>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
