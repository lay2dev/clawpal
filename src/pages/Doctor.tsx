import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { useApi } from "@/lib/use-api";
import { useInstance } from "@/lib/instance-context";
import { useDoctorAgent } from "@/lib/use-doctor-agent";
import type {
  RescuePrimaryDiagnosisResult,
  RescuePrimaryIssue,
  RescuePrimaryRepairResult,
  SshHost,
} from "@/lib/types";
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { DoctorChat } from "@/components/DoctorChat";

interface DoctorProps {
  sshHosts: SshHost[];
}

type RescueMessageTone = "info" | "success" | "error";

interface RescueUiState {
  activating: boolean;
  deactivating: boolean;
  unsetting: boolean;
  statusChecking: boolean;
  configured: boolean | null;
  profile: string;
  port: number | null;
  message: string | null;
  messageTone: RescueMessageTone;
}

interface PrimaryRecoveryState {
  checkLoading: boolean;
  checkResult: RescuePrimaryDiagnosisResult | null;
  checkError: string | null;
  repairing: boolean;
  repairingIssueId: string | null;
  repairResult: RescuePrimaryRepairResult | null;
  repairError: string | null;
}

const createInitialRescueUiState = (): RescueUiState => ({
  activating: false,
  deactivating: false,
  unsetting: false,
  statusChecking: false,
  configured: null,
  profile: "rescue",
  port: null,
  message: null,
  messageTone: "info",
});

const createInitialPrimaryRecoveryState = (): PrimaryRecoveryState => ({
  checkLoading: false,
  checkResult: null,
  checkError: null,
  repairing: false,
  repairingIssueId: null,
  repairResult: null,
  repairError: null,
});

export function Doctor({ sshHosts }: DoctorProps) {
  const { t } = useTranslation();
  const ua = useApi();
  const { instanceId, isRemote, isConnected } = useInstance();
  const doctor = useDoctorAgent();

  // Agent source: an instance id ("local" / host uuid) or "remote" (hosted doctor)
  const [agentSource, setAgentSource] = useState("remote");
  const [diagnosing, setDiagnosing] = useState(false);
  const selectableSources = [
    ...(doctor.target !== "local" ? ["local"] : []),
    ...sshHosts.filter((h) => h.id !== doctor.target).map((h) => h.id),
  ];
  const canStartDiagnosis = selectableSources.includes(agentSource);

  // Full-auto confirmation dialog
  const [fullAutoConfirmOpen, setFullAutoConfirmOpen] = useState(false);

  // Logs state
  const [logsOpen, setLogsOpen] = useState(false);
  const [logsSource, setLogsSource] = useState<"clawpal" | "gateway">("clawpal");
  const [logsTab, setLogsTab] = useState<"app" | "error">("app");
  const [logsContent, setLogsContent] = useState("");
  const [logsLoading, setLogsLoading] = useState(false);
  const logsContentRef = useRef<HTMLPreElement>(null);
  const [rescueState, setRescueState] = useState<RescueUiState>(createInitialRescueUiState);
  const [primaryState, setPrimaryState] = useState<PrimaryRecoveryState>(createInitialPrimaryRecoveryState);

  const {
    activating: rescueActivating,
    deactivating: rescueDeactivating,
    unsetting: rescueUnsetting,
    statusChecking: rescueStatusChecking,
    configured: rescueConfigured,
    profile: rescueProfile,
    port: rescuePort,
    message: rescueMessage,
    messageTone: rescueMessageTone,
  } = rescueState;
  const {
    checkLoading: primaryCheckLoading,
    checkResult: primaryCheckResult,
    checkError: primaryCheckError,
    repairing: primaryRepairing,
    repairingIssueId: primaryRepairingIssueId,
    repairResult: primaryRepairResult,
    repairError: primaryRepairError,
  } = primaryState;

  const updateRescueState = (patch: Partial<RescueUiState>) => {
    setRescueState((prev) => ({ ...prev, ...patch }));
  };

  const updatePrimaryState = (patch: Partial<PrimaryRecoveryState>) => {
    setPrimaryState((prev) => ({ ...prev, ...patch }));
  };

  // Reset doctor agent when switching instances
  useEffect(() => {
    doctor.reset();
    doctor.disconnect();
    setRescueState(createInitialRescueUiState());
    setPrimaryState(createInitialPrimaryRecoveryState());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId]);

  // Auto-infer target from active instance tab
  useEffect(() => {
    if (isRemote) {
      doctor.setTarget(instanceId);
    } else {
      doctor.setTarget("local");
    }
  }, [instanceId, isRemote, doctor.setTarget]);

  // Keep selected source valid when target/hosts change.
  useEffect(() => {
    if (canStartDiagnosis) return;
    if (selectableSources.length > 0) {
      setAgentSource(selectableSources[0]);
    }
  }, [canStartDiagnosis, selectableSources]);

  const handleStartDiagnosis = async () => {
    setDiagnosing(true);
    try {
      let url: string;
      let credentials;
      let agentId = "main";
      if (agentSource === "remote") {
        url = "wss://doctor.openclaw.ai";
      } else if (agentSource === "local") {
        url = "ws://localhost:18789";
      } else {
        // Remote gateway: ensure SSH connected, read credentials, tunnel
        const status = await api.sshStatus(agentSource);
        if (status !== "connected") {
          await api.sshConnect(agentSource);
        }
        credentials = await api.doctorReadRemoteCredentials(agentSource);
        // Get the first agent ID from the remote gateway
        const agents = await api.remoteListAgentsOverview(agentSource);
        if (agents.length > 0) {
          agentId = agents[0].id;
        }
        const localPort = await api.doctorPortForward(agentSource);
        url = `ws://localhost:${localPort}`;
      }

      const isRemoteGateway = agentSource !== "local" && agentSource !== "remote";
      try {
        await doctor.connect(url, credentials, isRemoteGateway ? agentSource : undefined);
      } catch (connectErr) {
        // Auto-fix NOT_PAIRED: approve pending device requests via SSH and retry
        if (String(connectErr).includes("NOT_PAIRED") && isRemoteGateway) {
          const approved = await api.doctorAutoPair(agentSource);
          if (approved > 0) {
            await doctor.connect(url, credentials, agentSource);
          } else {
            throw connectErr;
          }
        } else {
          throw connectErr;
        }
      }

      // Brief delay after bridge connection so the gateway propagates the
      // node's registered commands (system.run) to the agent's tool list.
      // Without this, the agent may start before it knows about our tools.
      await new Promise((r) => setTimeout(r, 800));

      const context = doctor.target === "local"
        ? await ua.collectDoctorContext()
        : await ua.collectDoctorContextRemote(doctor.target);

      await doctor.startDiagnosis(context, agentId);
    } catch {
      // Error is surfaced via doctor.error state from the hook
    } finally {
      setDiagnosing(false);
    }
  };

  const handleStopDiagnosis = async () => {
    await doctor.disconnect();
    doctor.reset();
  };

  // Logs helpers
  const fetchLog = (source: "clawpal" | "gateway", which: "app" | "error") => {
    setLogsLoading(true);
    const fn = source === "clawpal"
      ? (which === "app" ? ua.readAppLog : ua.readErrorLog)
      : (which === "app" ? ua.readGatewayLog : ua.readGatewayErrorLog);
    fn(500)
      .then((text) => {
        setLogsContent(text);
        setTimeout(() => {
          if (logsContentRef.current) {
            logsContentRef.current.scrollTop = logsContentRef.current.scrollHeight;
          }
        }, 50);
      })
      .catch(() => setLogsContent(""))
      .finally(() => setLogsLoading(false));
  };

  const openLogs = (source: "clawpal" | "gateway") => {
    setLogsSource(source);
    setLogsTab("app");
    setLogsOpen(true);
  };

  const refreshRescueStatus = async (isCancelled?: () => boolean) => {
    const cancelled = () => isCancelled?.() ?? false;
    if (isRemote && !isConnected) {
      if (cancelled()) return;
      updateRescueState({
        configured: null,
        port: null,
        message: t("doctor.rescueBotConnectRequired"),
        messageTone: "info",
      });
      return;
    }

    updateRescueState({ statusChecking: true });
    try {
      const result = await ua.manageRescueBot("status");
      if (cancelled()) return;
      updateRescueState({
        configured: result.wasAlreadyConfigured,
        profile: result.profile,
        port: result.wasAlreadyConfigured ? result.rescuePort : null,
        message: result.wasAlreadyConfigured
          ? t("doctor.rescueBotAlreadyConfiguredState", {
            profile: result.profile,
            port: result.rescuePort,
          })
          : t("doctor.rescueBotNotConfigured"),
        messageTone: "info",
      });
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      if (cancelled()) return;
      updateRescueState({
        configured: null,
        port: null,
        message: t("doctor.rescueBotStatusCheckFailed", { error: text }),
        messageTone: "error",
      });
    } finally {
      if (cancelled()) return;
      updateRescueState({ statusChecking: false });
    }
  };

  const handleActivateRescueBot = async () => {
    if (isRemote && !isConnected) {
      updateRescueState({
        message: t("doctor.rescueBotConnectRequired"),
        messageTone: "error",
      });
      return;
    }
    updateRescueState({
      activating: true,
      message: null,
      messageTone: "info",
    });
    try {
      const result = await ua.manageRescueBot("activate");
      updateRescueState({
        configured: true,
        profile: result.profile,
        port: result.rescuePort,
        message: t("doctor.rescueBotActivated", {
          profile: result.profile,
          port: result.rescuePort,
        }),
        messageTone: "success",
      });
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      if (text.includes("Gateway restart timed out")) {
        updateRescueState({
          message: t("doctor.rescueBotFailedTimeout", { error: text }),
          messageTone: "error",
        });
      } else {
        updateRescueState({
          message: t("doctor.rescueBotFailed", { error: text }),
          messageTone: "error",
        });
      }
    } finally {
      updateRescueState({ activating: false });
    }
  };

  const handleDeactivateRescueBot = async () => {
    if (isRemote && !isConnected) {
      updateRescueState({
        message: t("doctor.rescueBotConnectRequired"),
        messageTone: "error",
      });
      return;
    }
    updateRescueState({
      deactivating: true,
      message: null,
      messageTone: "info",
    });
    try {
      const result = await ua.manageRescueBot("deactivate");
      if (result.wasAlreadyConfigured) {
        updateRescueState({
          profile: result.profile,
          configured: true,
          port: result.rescuePort,
          message: t("doctor.rescueBotDeactivated", { profile: result.profile }),
          messageTone: "success",
        });
      } else {
        updateRescueState({
          profile: result.profile,
          configured: false,
          port: null,
          message: t("doctor.rescueBotAlreadyNotConfigured"),
          messageTone: "info",
        });
      }
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      updateRescueState({
        message: t("doctor.rescueBotDeactivateFailed", { error: text }),
        messageTone: "error",
      });
    } finally {
      updateRescueState({ deactivating: false });
    }
  };

  const handleUnsetRescueBot = async () => {
    if (isRemote && !isConnected) {
      updateRescueState({
        message: t("doctor.rescueBotConnectRequired"),
        messageTone: "error",
      });
      return;
    }
    updateRescueState({
      unsetting: true,
      message: null,
      messageTone: "info",
    });
    try {
      const result = await ua.manageRescueBot("unset");
      if (result.wasAlreadyConfigured) {
        updateRescueState({
          profile: result.profile,
          configured: false,
          port: null,
          message: t("doctor.rescueBotUnset", { profile: result.profile }),
          messageTone: "success",
        });
      } else {
        updateRescueState({
          profile: result.profile,
          configured: false,
          port: null,
          message: t("doctor.rescueBotAlreadyNotConfigured"),
          messageTone: "info",
        });
      }
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      updateRescueState({
        message: t("doctor.rescueBotUnsetFailed", { error: text }),
        messageTone: "error",
      });
    } finally {
      updateRescueState({ unsetting: false });
    }
  };

  const handleCheckPrimaryViaRescue = async () => {
    if (isRemote && !isConnected) {
      updatePrimaryState({ checkError: t("doctor.rescueBotConnectRequired") });
      return;
    }
    updatePrimaryState({
      checkLoading: true,
      checkError: null,
      repairError: null,
      repairResult: null,
    });
    try {
      const result = await ua.diagnosePrimaryViaRescue("primary", rescueProfile);
      updatePrimaryState({ checkResult: result });
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      updatePrimaryState({
        checkResult: null,
        checkError: t("doctor.primaryCheckFailed", { error: text }),
      });
    } finally {
      updatePrimaryState({ checkLoading: false });
    }
  };

  const primaryStatusLabel = (status: RescuePrimaryDiagnosisResult["status"]) => {
    if (status === "healthy") return t("doctor.primaryStatusHealthy");
    if (status === "degraded") return t("doctor.primaryStatusDegraded");
    return t("doctor.primaryStatusBroken");
  };

  const formatCheckedAt = (checkedAt: string) => {
    const value = new Date(checkedAt);
    if (Number.isNaN(value.getTime())) return checkedAt;
    return value.toLocaleString();
  };

  const countSafeFixableIssues = (result: RescuePrimaryDiagnosisResult | null) =>
    result?.issues.filter((issue) => issue.source === "primary" && issue.autoFixable).length ?? 0;

  const handleRepairPrimaryViaRescue = async () => {
    if (isRemote && !isConnected) {
      updatePrimaryState({ repairError: t("doctor.rescueBotConnectRequired") });
      return;
    }
    updatePrimaryState({
      repairing: true,
      repairingIssueId: null,
      repairError: null,
      repairResult: null,
    });
    try {
      const selectedIssueIds =
        primaryCheckResult?.issues
          .filter((issue) => issue.source === "primary" && issue.autoFixable)
          .map((issue) => issue.id) ?? [];
      const result = await ua.repairPrimaryViaRescue(
        "primary",
        rescueProfile,
        selectedIssueIds.length > 0 ? selectedIssueIds : undefined,
      );
      updatePrimaryState({
        repairResult: result,
        checkResult: result.after,
        checkError: null,
      });
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      updatePrimaryState({
        repairResult: null,
        repairError: t("doctor.primaryRepairFailed", { error: text }),
      });
    } finally {
      updatePrimaryState({
        repairing: false,
        repairingIssueId: null,
      });
    }
  };

  const handleRepairPrimaryIssue = async (issue: RescuePrimaryIssue) => {
    if (!issue.autoFixable || issue.source !== "primary") {
      return;
    }
    if (isRemote && !isConnected) {
      updatePrimaryState({ repairError: t("doctor.rescueBotConnectRequired") });
      return;
    }
    updatePrimaryState({
      repairing: true,
      repairingIssueId: issue.id,
      repairError: null,
      repairResult: null,
    });
    try {
      const result = await ua.repairPrimaryViaRescue("primary", rescueProfile, [issue.id]);
      updatePrimaryState({
        repairResult: result,
        checkResult: result.after,
        checkError: null,
      });
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      updatePrimaryState({
        repairResult: null,
        repairError: t("doctor.primaryRepairFailed", { error: text }),
      });
    } finally {
      updatePrimaryState({
        repairing: false,
        repairingIssueId: null,
      });
    }
  };

  useEffect(() => {
    let cancelled = false;
    void refreshRescueStatus(() => cancelled);
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId, isRemote, isConnected]);

  useEffect(() => {
    if (logsOpen) fetchLog(logsSource, logsTab);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [logsOpen, logsSource, logsTab]);

  return (
    <section>
      <h2 className="text-2xl font-bold mb-4">{t("doctor.title")}</h2>

      <Card className="mb-4 gap-2 py-4">
        <CardHeader className="pb-0">
          <CardTitle className="text-base">{t("doctor.rescueBotTitle")}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-between gap-3 flex-wrap">
            <p className="text-sm text-muted-foreground">{t("doctor.rescueBotHint")}</p>
            <div className="flex items-center gap-2">
              <Button
                variant="default"
                size="sm"
                onClick={handleActivateRescueBot}
                disabled={
                  rescueActivating
                  || rescueDeactivating
                  || rescueUnsetting
                  || rescueStatusChecking
                  || (isRemote && !isConnected)
                }
              >
                {rescueActivating ? t("doctor.activatingRescueBot") : t("doctor.activateRescueBot")}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={handleDeactivateRescueBot}
                disabled={
                  rescueActivating
                  || rescueDeactivating
                  || rescueUnsetting
                  || rescueStatusChecking
                  || rescueConfigured !== true
                  || (isRemote && !isConnected)
                }
              >
                {rescueDeactivating ? t("doctor.deactivatingRescueBot") : t("doctor.deactivateRescueBot")}
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={handleUnsetRescueBot}
                disabled={
                  rescueActivating
                  || rescueDeactivating
                  || rescueUnsetting
                  || rescueStatusChecking
                  || rescueConfigured !== true
                  || (isRemote && !isConnected)
                }
              >
                {rescueUnsetting ? t("doctor.unsettingRescueBot") : t("doctor.unsetRescueBot")}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  void refreshRescueStatus();
                }}
                disabled={
                  rescueActivating
                  || rescueDeactivating
                  || rescueUnsetting
                  || rescueStatusChecking
                  || (isRemote && !isConnected)
                }
              >
                {rescueStatusChecking ? t("doctor.rescueBotChecking") : t("doctor.refresh")}
              </Button>
            </div>
          </div>
          {rescueMessage && (
            <div
              className={`mt-3 rounded-md border px-3 py-2 text-sm ${
                rescueMessageTone === "error"
                  ? "border-destructive/40 bg-destructive/10 text-destructive"
                  : rescueMessageTone === "success"
                    ? "border-emerald-500/40 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
                    : "border-border/50 bg-muted/40 text-muted-foreground"
              }`}
            >
              <div>{rescueMessage}</div>
              {rescueMessageTone === "error" && (
                <div className="mt-2">
                  <Button variant="outline" size="sm" onClick={() => openLogs("gateway")}>
                    {t("doctor.viewGatewayLogs")}
                  </Button>
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      <Card className="mb-4 gap-2 py-4">
        <CardHeader className="pb-0">
          <CardTitle className="text-base">{t("doctor.primaryRecoveryTitle")}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-between gap-3 flex-wrap">
            <p className="text-sm text-muted-foreground">{t("doctor.primaryRecoveryHint")}</p>
            <div className="flex items-center gap-2">
              <Button
                variant="default"
                size="sm"
                onClick={handleCheckPrimaryViaRescue}
                disabled={primaryCheckLoading || primaryRepairing || (isRemote && !isConnected)}
              >
                {primaryCheckLoading
                  ? t("doctor.primaryChecking")
                  : t("doctor.primaryCheckNow")}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={handleRepairPrimaryViaRescue}
                disabled={
                  primaryCheckLoading
                  || primaryRepairing
                  || !primaryCheckResult
                  || (isRemote && !isConnected)
                }
              >
                {primaryRepairing
                  ? t("doctor.primaryRepairing")
                  : t("doctor.primaryRepairNow", { count: countSafeFixableIssues(primaryCheckResult) })}
              </Button>
            </div>
          </div>
          {primaryCheckError && (
            <div className="mt-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              <div>{primaryCheckError}</div>
              <div className="mt-2">
                <Button variant="outline" size="sm" onClick={() => openLogs("gateway")}>
                  {t("doctor.viewGatewayLogs")}
                </Button>
              </div>
            </div>
          )}
          {primaryRepairError && (
            <div className="mt-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              <div>{primaryRepairError}</div>
              <div className="mt-2">
                <Button variant="outline" size="sm" onClick={() => openLogs("gateway")}>
                  {t("doctor.viewGatewayLogs")}
                </Button>
              </div>
            </div>
          )}
          {primaryCheckResult && (
            <div className="mt-3 rounded-md border border-border/60 bg-muted/20 px-3 py-3">
              <div className="flex items-center justify-between gap-2 flex-wrap">
                <div className="text-sm">
                  {t("doctor.primaryCheckedAt", { time: formatCheckedAt(primaryCheckResult.checkedAt) })}
                </div>
                <Badge
                  variant={primaryCheckResult.status === "healthy" ? "outline" : "destructive"}
                  className={primaryCheckResult.status === "healthy" ? "border-emerald-500/40 text-emerald-700 dark:text-emerald-300" : undefined}
                >
                  {primaryStatusLabel(primaryCheckResult.status)}
                </Badge>
              </div>
              <div className="mt-3 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                {t("doctor.primaryChecks")}
              </div>
              <div className="mt-2 grid gap-2">
                {primaryCheckResult.checks.map((check) => (
                  <div key={check.id} className="rounded-md border border-border/50 bg-background/60 p-2">
                    <div className="flex items-center justify-between gap-2">
                      <div className="flex items-center gap-2">
                        <div className="text-sm">{check.title}</div>
                        {!check.ok && check.id === "rescue.profile.configured" && (
                          <Button
                            variant="outline"
                            size="sm"
                            className="h-6 px-2 text-[11px]"
                            onClick={handleActivateRescueBot}
                            disabled={
                              rescueActivating
                              || rescueDeactivating
                              || rescueUnsetting
                              || rescueStatusChecking
                              || (isRemote && !isConnected)
                            }
                          >
                            {rescueActivating ? t("doctor.activatingRescueBot") : t("doctor.activateRescueBot")}
                          </Button>
                        )}
                        {!check.ok && check.id.startsWith("primary.") && countSafeFixableIssues(primaryCheckResult) > 0 && (
                          <Button
                            variant="outline"
                            size="sm"
                            className="h-6 px-2 text-[11px]"
                            onClick={handleRepairPrimaryViaRescue}
                            disabled={primaryCheckLoading || primaryRepairing || (isRemote && !isConnected)}
                          >
                            {primaryRepairing ? t("doctor.primaryRepairing") : t("doctor.primaryQuickFix")}
                          </Button>
                        )}
                      </div>
                      <Badge variant={check.ok ? "outline" : "destructive"} className="text-[10px]">
                        {check.ok ? t("doctor.primaryCheckPass") : t("doctor.primaryCheckFail")}
                      </Badge>
                    </div>
                    <div className="mt-1 text-xs text-muted-foreground">{check.detail}</div>
                  </div>
                ))}
              </div>
              <div className="mt-3 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                {t("doctor.primaryIssues")}
              </div>
              {primaryCheckResult.issues.length === 0 ? (
                <div className="mt-2 text-sm text-emerald-700 dark:text-emerald-300">
                  {t("doctor.primaryNoIssues")}
                </div>
              ) : (
                <div className="mt-2 grid gap-2">
                  {primaryCheckResult.issues.map((issue) => (
                    <div key={issue.id} className="rounded-md border border-destructive/30 bg-destructive/5 p-2">
                      <div className="flex items-center justify-between gap-2">
                        <div className="flex items-center gap-2">
                          <div className="text-sm">{issue.message}</div>
                          {issue.source === "primary" && issue.autoFixable && (
                            <Button
                              variant="outline"
                              size="sm"
                              className="h-6 px-2 text-[11px]"
                              onClick={() => {
                                void handleRepairPrimaryIssue(issue);
                              }}
                              disabled={primaryCheckLoading || primaryRepairing || (isRemote && !isConnected)}
                            >
                              {primaryRepairing && primaryRepairingIssueId === issue.id
                                ? t("doctor.primaryIssueFixing")
                                : t("doctor.primaryIssueFix")}
                            </Button>
                          )}
                        </div>
                        <div className="flex items-center gap-1">
                          <Badge variant="outline" className="text-[10px]">
                            {issue.source === "rescue"
                              ? t("doctor.primaryIssueSourceRescue")
                              : t("doctor.primaryIssueSourcePrimary")}
                          </Badge>
                          <Badge variant={issue.severity === "error" ? "destructive" : "outline"} className="text-[10px]">
                            {issue.severity}
                          </Badge>
                        </div>
                      </div>
                      {issue.fixHint && (
                        <div className="mt-1 text-xs text-muted-foreground">{issue.fixHint}</div>
                      )}
                    </div>
                  ))}
                </div>
              )}
              {primaryRepairResult && (
                <div className="mt-4 rounded-md border border-border/60 bg-background/70 p-3">
                  <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                    {t("doctor.primaryRepairSummary")}
                  </div>
                  <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
                    <Badge variant="outline">
                      {t("doctor.primaryRepairSelected", { count: primaryRepairResult.selectedIssueIds.length })}
                    </Badge>
                    <Badge variant="outline" className="border-emerald-500/40 text-emerald-700 dark:text-emerald-300">
                      {t("doctor.primaryRepairApplied", { count: primaryRepairResult.appliedIssueIds.length })}
                    </Badge>
                    <Badge variant="outline">
                      {t("doctor.primaryRepairSkipped", { count: primaryRepairResult.skippedIssueIds.length })}
                    </Badge>
                    <Badge variant={primaryRepairResult.failedIssueIds.length > 0 ? "destructive" : "outline"}>
                      {t("doctor.primaryRepairFailedCount", { count: primaryRepairResult.failedIssueIds.length })}
                    </Badge>
                  </div>
                  <div className="mt-2 text-xs text-muted-foreground">
                    {t("doctor.primaryRecheckedAt", { time: formatCheckedAt(primaryRepairResult.after.checkedAt) })}
                  </div>
                  <div className="mt-3 grid gap-2">
                    {primaryRepairResult.steps.map((step) => (
                      <div key={step.id} className="rounded-md border border-border/50 bg-muted/20 p-2">
                        <div className="flex items-center justify-between gap-2">
                          <div className="text-sm">{step.title}</div>
                          <Badge variant={step.ok ? "outline" : "destructive"} className="text-[10px]">
                            {step.ok ? t("doctor.primaryCheckPass") : t("doctor.primaryCheckFail")}
                          </Badge>
                        </div>
                        <div className="mt-1 text-xs text-muted-foreground">{step.detail}</div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      <Card className="gap-2 py-4">
        <CardHeader className="pb-0">
          <div className="flex items-center justify-between">
            <CardTitle className="text-base">{t("doctor.agentSource")}</CardTitle>
            <div className="flex items-center gap-1">
              <Button variant="ghost" size="sm" onClick={() => openLogs("clawpal")}>
                {t("doctor.clawpalLogs")}
              </Button>
              <Button variant="ghost" size="sm" onClick={() => openLogs("gateway")}>
                {t("doctor.gatewayLogs")}
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {!doctor.connected && doctor.messages.length === 0 ? (
            <>
              {/* Source radio — instance gateways (excluding current target) + remote doctor */}
              <div className="text-sm text-muted-foreground mb-2">{t("doctor.agentSourceHint")}</div>
              <div className="flex items-center gap-4 mb-4 flex-wrap">
                {doctor.target !== "local" && (
                  <label className="flex items-center gap-1.5 text-sm cursor-pointer">
                    <input
                      type="radio"
                      name="agentSource"
                      value="local"
                      checked={agentSource === "local"}
                      onChange={() => setAgentSource("local")}
                      className="accent-primary"
                    />
                    {t("instance.local")}
                  </label>
                )}
                {sshHosts
                  .filter((h) => h.id !== doctor.target)
                  .map((h) => (
                    <label key={h.id} className="flex items-center gap-1.5 text-sm cursor-pointer">
                      <input
                        type="radio"
                        name="agentSource"
                        value={h.id}
                        checked={agentSource === h.id}
                        onChange={() => setAgentSource(h.id)}
                        className="accent-primary"
                      />
                      {h.label || h.host}
                    </label>
                  ))}
                <label className="flex items-center gap-1.5 text-sm cursor-not-allowed text-muted-foreground">
                  <input
                    type="radio"
                    name="agentSource"
                    value="remote"
                    disabled
                    className="accent-primary"
                  />
                  {t("doctor.remoteDoctor")}
                  <span className="text-xs">(coming soon)</span>
                </label>
              </div>
              {doctor.error && (
                <div className="mb-3 text-sm text-destructive">
                  {doctor.error}
                  {doctor.error.includes("NOT_PAIRED") && (
                    <p className="mt-1 text-muted-foreground">
                      {t("doctor.notPairedHint", {
                        host: agentSource === "local"
                          ? "localhost"
                          : sshHosts.find((h) => h.id === agentSource)?.label || agentSource,
                      })}
                    </p>
                  )}
                </div>
              )}
              <Button onClick={handleStartDiagnosis} disabled={diagnosing || !canStartDiagnosis}>
                {diagnosing ? t("doctor.connecting") : t("doctor.startDiagnosis")}
              </Button>
            </>
          ) : !doctor.connected && doctor.messages.length > 0 ? (
            <>
              {/* Disconnected mid-session — show chat with reconnect banner */}
              <div className="flex items-center justify-between mb-3 p-2 rounded-md bg-destructive/10 border border-destructive/20">
                <span className="text-sm text-destructive">
                  {doctor.error || t("doctor.disconnected")}
                </span>
                <div className="flex items-center gap-2">
                  <Button size="sm" onClick={() => doctor.reconnect()}>
                    {t("doctor.reconnect")}
                  </Button>
                  <Button variant="outline" size="sm" onClick={handleStopDiagnosis}>
                    {t("doctor.stopDiagnosis")}
                  </Button>
                </div>
              </div>
              <DoctorChat
                messages={doctor.messages}
                loading={false}
                error={null}
                connected={false}
                onSendMessage={doctor.sendMessage}
                onApproveInvoke={doctor.approveInvoke}
                onRejectInvoke={doctor.rejectInvoke}
              />
            </>
          ) : (
            <>
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="text-xs">
                    {agentSource === "remote"
                      ? t("doctor.remoteDoctor")
                      : agentSource === "local"
                        ? t("instance.local")
                        : sshHosts.find((h) => h.id === agentSource)?.label || agentSource}
                  </Badge>
                  <Badge variant="outline" className="text-xs flex items-center gap-1.5">
                    <span className={`inline-block w-1.5 h-1.5 rounded-full ${doctor.bridgeConnected ? "bg-emerald-500" : "bg-muted-foreground/40"}`} />
                    {doctor.bridgeConnected ? t("doctor.bridgeConnected") : t("doctor.bridgeDisconnected")}
                  </Badge>
                </div>
                <div className="flex items-center gap-2">
                  <label className="flex items-center gap-1.5 text-xs cursor-pointer select-none">
                    <input
                      type="checkbox"
                      checked={doctor.fullAuto}
                      onChange={(e) => {
                        if (e.target.checked) {
                          setFullAutoConfirmOpen(true);
                        } else {
                          doctor.setFullAuto(false);
                        }
                      }}
                      className="accent-primary"
                    />
                    {t("doctor.fullAuto")}
                  </label>
                  <Button variant="outline" size="sm" onClick={handleStopDiagnosis}>
                    {t("doctor.stopDiagnosis")}
                  </Button>
                </div>
              </div>
              <DoctorChat
                messages={doctor.messages}
                loading={doctor.loading}
                error={doctor.error}
                connected={doctor.connected}
                onSendMessage={doctor.sendMessage}
                onApproveInvoke={doctor.approveInvoke}
                onRejectInvoke={doctor.rejectInvoke}
              />
            </>
          )}
        </CardContent>
      </Card>

      {/* Full-Auto Confirmation */}
      <Dialog open={fullAutoConfirmOpen} onOpenChange={setFullAutoConfirmOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("doctor.fullAutoTitle")}</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">{t("doctor.fullAutoWarning")}</p>
          <div className="flex justify-end gap-2 mt-4">
            <Button variant="outline" size="sm" onClick={() => setFullAutoConfirmOpen(false)}>
              {t("doctor.cancel")}
            </Button>
            <Button variant="destructive" size="sm" onClick={() => {
              doctor.setFullAuto(true);
              setFullAutoConfirmOpen(false);
            }}>
              {t("doctor.fullAutoConfirm")}
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      {/* Logs Dialog */}
      <Dialog open={logsOpen} onOpenChange={setLogsOpen}>
        <DialogContent className="sm:max-w-2xl max-h-[80vh] flex flex-col">
          <DialogHeader>
            <DialogTitle>
              {logsSource === "clawpal" ? t("doctor.clawpalLogs") : t("doctor.gatewayLogs")}
            </DialogTitle>
          </DialogHeader>
          <div className="flex items-center gap-2 mb-2">
            <Button
              variant={logsTab === "app" ? "default" : "outline"}
              size="sm"
              onClick={() => setLogsTab("app")}
            >
              {t("doctor.appLog")}
            </Button>
            <Button
              variant={logsTab === "error" ? "default" : "outline"}
              size="sm"
              onClick={() => setLogsTab("error")}
            >
              {t("doctor.errorLog")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => fetchLog(logsSource, logsTab)}
              disabled={logsLoading}
            >
              {t("doctor.refreshLogs")}
            </Button>
          </div>
          <pre
            ref={logsContentRef}
            className="flex-1 min-h-[300px] max-h-[60vh] overflow-auto rounded-md border bg-muted p-3 text-xs font-mono whitespace-pre-wrap break-all"
          >
            {logsContent || t("doctor.noLogs")}
          </pre>
        </DialogContent>
      </Dialog>
    </section>
  );
}
