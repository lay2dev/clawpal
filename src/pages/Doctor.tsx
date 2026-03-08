import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangleIcon,
  CheckCircle2Icon,
  CircleDashedIcon,
  FileTextIcon,
  LoaderCircleIcon,
  MoreHorizontalIcon,
  PlayIcon,
  PauseCircleIcon,
  PauseIcon,
  RefreshCwIcon,
  StethoscopeIcon,
  Trash2Icon,
} from "lucide-react";
import { toast } from "sonner";

import { DoctorLogsDialog } from "@/components/DoctorLogsDialog";
import { DoctorRecoveryOverview } from "@/components/DoctorRecoveryOverview";
import { RescueAsciiHeader } from "@/components/RescueAsciiHeader";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { useInstance } from "@/lib/instance-context";
import {
  buildStatusProgressLines,
  buildCheckProgressLines,
  buildFixProgressLines,
  getPrimaryRescueAction,
  getPrimaryRescueActionIcon,
  getIdleRescueProgress,
  isIconOnlyPrimaryRescueAction,
  normalizeRescueManageResultAfterAction,
  shouldRefreshStatusAfterAction,
  shouldShowPrimaryRecovery,
} from "@/lib/rescueBotUi";
import type {
  RescueBotAction,
  RescueBotManageResult,
  RescueBotRuntimeState,
  RescuePrimaryDiagnosisResult,
  RescuePrimaryRepairResult,
} from "@/lib/types";
import { useApi } from "@/lib/use-api";

interface RescueUiState {
  pendingAction: RescueBotAction | null;
  statusChecking: boolean;
  runtimeState: RescueBotRuntimeState;
  configured: boolean;
  active: boolean;
  profile: string;
  port: number | null;
  error: string | null;
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

interface DoctorProps {
}

const createInitialRescueUiState = (): RescueUiState => ({
  pendingAction: null,
  statusChecking: false,
  runtimeState: "checking",
  configured: false,
  active: false,
  profile: "rescue",
  port: null,
  error: null,
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

function useRotatingLine(active: boolean, lines: string[]) {
  const [index, setIndex] = useState(0);

  useEffect(() => {
    if (!active || lines.length === 0) {
      setIndex(0);
      return;
    }
    setIndex(0);
    const timer = window.setInterval(() => {
      setIndex((current) => (current + 1) % lines.length);
    }, 1400);
    return () => window.clearInterval(timer);
  }, [active, lines]);

  if (!active || lines.length === 0) {
    return null;
  }
  return {
    line: lines[index] ?? null,
    index,
    total: lines.length,
  };
}

function RescueStatusIndicator({
  state,
  title,
}: {
  state: RescueBotRuntimeState;
  title: string;
}) {
  if (state === "active") {
    return (
      <div
        className="inline-flex size-9 items-center justify-center rounded-full border border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
        title={title}
        aria-label={title}
      >
        <CheckCircle2Icon className="size-4" />
      </div>
    );
  }
  if (state === "configured_inactive") {
    return (
      <div
        className="inline-flex size-9 items-center justify-center rounded-full border border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300"
        title={title}
        aria-label={title}
      >
        <PauseCircleIcon className="size-4" />
      </div>
    );
  }
  if (state === "error") {
    return (
      <div
        className="inline-flex size-9 items-center justify-center rounded-full border border-destructive/30 bg-destructive/10 text-destructive"
        title={title}
        aria-label={title}
      >
        <AlertTriangleIcon className="size-4" />
      </div>
    );
  }
  if (state === "checking") {
    return (
      <div
        className="inline-flex size-9 items-center justify-center rounded-full border border-border/60 bg-muted/40 text-muted-foreground"
        title={title}
        aria-label={title}
      >
        <LoaderCircleIcon className="size-4 animate-spin" />
      </div>
    );
  }
  return (
    <div
      className="inline-flex size-9 items-center justify-center rounded-full border border-border/60 bg-muted/40 text-muted-foreground"
      title={title}
      aria-label={title}
    >
      <CircleDashedIcon className="size-4" />
    </div>
  );
}

export function Doctor(_: DoctorProps) {
  const { t } = useTranslation();
  const ua = useApi();
  const { isRemote, isConnected } = useInstance();

  const [logsOpen, setLogsOpen] = useState(false);
  const [logsSource, setLogsSource] = useState<"clawpal" | "gateway" | "helper">("gateway");

  const [rescueState, setRescueState] = useState<RescueUiState>(createInitialRescueUiState);
  const [primaryState, setPrimaryState] = useState<PrimaryRecoveryState>(
    createInitialPrimaryRecoveryState,
  );

  const updateRescueState = (patch: Partial<RescueUiState>) => {
    setRescueState((prev) => ({ ...prev, ...patch }));
  };

  const updatePrimaryState = (patch: Partial<PrimaryRecoveryState>) => {
    setPrimaryState((prev) => ({ ...prev, ...patch }));
  };

  const applyRescueResult = useCallback((result: RescueBotManageResult) => {
    updateRescueState({
      runtimeState: result.runtimeState,
      configured: result.configured,
      active: result.active,
      profile: result.profile,
      port: result.configured ? result.rescuePort : null,
      error: null,
    });
    if (!result.active) {
      setPrimaryState(createInitialPrimaryRecoveryState());
    }
  }, []);

  const openLogs = useCallback((source: "clawpal" | "gateway" | "helper" = "gateway") => {
    setLogsSource(source);
    setLogsOpen(true);
  }, []);

  const refreshRescueStatus = useCallback(async (isCancelled?: () => boolean) => {
    const cancelled = () => isCancelled?.() ?? false;
    if (isRemote && !isConnected) {
      if (cancelled()) return;
      updateRescueState({
        statusChecking: false,
        runtimeState: "error",
        configured: false,
        active: false,
        port: null,
        error: t("doctor.rescueBotConnectRequired"),
      });
      return;
    }

    updateRescueState({ statusChecking: true, error: null });
    try {
      const result = await ua.manageRescueBot("status");
      if (cancelled()) return;
      applyRescueResult(result);
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      if (cancelled()) return;
      updateRescueState({
        runtimeState: "error",
        configured: false,
        active: false,
        port: null,
        error: t("doctor.rescueBotStatusCheckFailed", {
          defaultValue: "Failed to check Rescue Bot: {{error}}",
          error: text,
        }),
      });
    } finally {
      if (cancelled()) return;
      updateRescueState({ statusChecking: false });
    }
  }, [applyRescueResult, isConnected, isRemote, t, ua]);

  const runRescueAction = async (action: RescueBotAction) => {
    if (isRemote && !isConnected) {
      updateRescueState({ runtimeState: "error", error: t("doctor.rescueBotConnectRequired") });
      return;
    }

    updateRescueState({ pendingAction: action, error: null });
    try {
      const result = await ua.manageRescueBot(action);
      applyRescueResult(normalizeRescueManageResultAfterAction(action, result));
      const successText = (() => {
        switch (action) {
          case "set":
            return t("doctor.rescueBotSetSuccess", {
              defaultValue: "Recovery helper is ready.",
            });
          case "activate":
            return t("doctor.rescueBotActivateSuccess", {
              defaultValue: "Recovery helper is enabled.",
            });
          case "deactivate":
            return t("doctor.rescueBotDeactivateSuccess", {
              defaultValue: "Recovery helper is paused.",
            });
          case "unset":
            return t("doctor.rescueBotUnsetSuccess", {
              defaultValue: "Recovery helper setup was removed.",
            });
          default:
            return null;
        }
      })();
      if (shouldRefreshStatusAfterAction(action)) {
        updateRescueState({ statusChecking: true });
        try {
          const statusResult = await ua.manageRescueBot("status");
          applyRescueResult(statusResult);
        } catch (error) {
          const text = error instanceof Error ? error.message : String(error);
          updateRescueState({
            runtimeState: "error",
            error: t("doctor.rescueBotStatusCheckFailed", {
              defaultValue: "Failed to refresh helper status: {{error}}",
              error: text,
            }),
          });
          toast.error(
            t("doctor.rescueBotStatusCheckFailed", {
              defaultValue: "Failed to refresh helper status: {{error}}",
              error: text,
            }),
          );
          return;
        } finally {
          updateRescueState({ statusChecking: false });
        }
      }
      if (successText) {
        toast.success(successText);
      }
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      updateRescueState({
        runtimeState: "error",
        error: t("doctor.rescueBotActionFailed", {
          defaultValue: "Rescue Bot action failed: {{error}}",
          error: text,
        }),
      });
      toast.error(
        t("doctor.rescueBotActionFailed", {
          defaultValue: "Rescue Bot action failed: {{error}}",
          error: text,
        }),
      );
    } finally {
      updateRescueState({ pendingAction: null });
    }
  };

  const handleCheckPrimaryViaRescue = async () => {
    if (isRemote && !isConnected) {
      updatePrimaryState({ checkError: t("doctor.rescueBotConnectRequired") });
      return;
    }
    updatePrimaryState({
      checkLoading: true,
      checkResult: null,
      checkError: null,
      repairing: false,
      repairingIssueId: null,
      repairResult: null,
      repairError: null,
    });
    try {
      const result = await ua.diagnosePrimaryViaRescue("primary", rescueState.profile);
      updatePrimaryState({ checkResult: result });
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      updatePrimaryState({
        checkError: t("doctor.primaryCheckFailed", {
          defaultValue: "Primary recovery check failed: {{error}}",
          error: text,
        }),
      });
    } finally {
      updatePrimaryState({ checkLoading: false });
    }
  };

  const handleRepairPrimaryViaRescue = async (issueIds?: string[]) => {
    if (isRemote && !isConnected) {
      updatePrimaryState({ repairError: t("doctor.rescueBotConnectRequired") });
      return;
    }
    const selectedIssueIds =
      issueIds
      ?? primaryState.checkResult?.summary.selectedFixIssueIds
      ?? [];
    updatePrimaryState({
      repairing: true,
      repairingIssueId: issueIds?.length === 1 ? issueIds[0] : null,
      repairError: null,
      repairResult: null,
    });
    try {
      const result = await ua.repairPrimaryViaRescue(
        "primary",
        rescueState.profile,
        selectedIssueIds,
      );
      updatePrimaryState({
        repairResult: result,
        checkResult: result.after,
        checkError: null,
      });
      toast.success(
        t("doctor.primaryRepairSuccess", {
          defaultValue: "Applied {{count}} fix(es).",
          count: result.appliedIssueIds.length,
        }),
      );
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      updatePrimaryState({
        repairResult: null,
        repairError: t("doctor.primaryRepairFailed", {
          defaultValue: "Primary repair failed: {{error}}",
          error: text,
        }),
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
  }, [isConnected, isRemote, refreshRescueStatus]);

  const visibleRuntimeState: RescueBotRuntimeState =
    rescueState.pendingAction || rescueState.statusChecking
      ? "checking"
      : rescueState.runtimeState;

  const primaryAction = getPrimaryRescueAction(rescueState.runtimeState);
  const primaryActionLabel = (() => {
    if (primaryAction === "activate") {
      return t("doctor.rescueBotActivate", { defaultValue: "Play" });
    }
    return t("doctor.rescueBotDeactivate", { defaultValue: "Pause" });
  })();
  const primaryActionBusyLabel = (() => {
    if (primaryAction === "activate") {
      return t("doctor.activatingRescueBot", { defaultValue: "Starting..." });
    }
    return t("doctor.deactivatingRescueBot", { defaultValue: "Pausing..." });
  })();
  const statusLabel = (() => {
    switch (visibleRuntimeState) {
      case "active":
        return t("doctor.rescueBotStateActive", { defaultValue: "Helper is enabled" });
      case "configured_inactive":
        return t("doctor.rescueBotStateInactive", {
          defaultValue: "Helper is paused",
        });
      case "checking":
        return t("doctor.rescueBotChecking", { defaultValue: "Checking helper status" });
      case "error":
        return t("doctor.rescueBotStateError", {
          defaultValue: "Helper needs attention",
        });
      default:
        return t("doctor.rescueBotStateUnset", {
          defaultValue: "Helper is not set up",
        });
    }
  })();

  const statusProgress = useRotatingLine(
    rescueState.pendingAction !== null || rescueState.statusChecking,
    useMemo(() => buildStatusProgressLines(), []),
  );
  const checkProgress = useRotatingLine(
    primaryState.checkLoading,
    useMemo(() => buildCheckProgressLines(), []),
  );
  const fixProgress = useRotatingLine(
    primaryState.repairing,
    useMemo(
      () => buildFixProgressLines(primaryState.checkResult?.sections ?? []),
      [primaryState.checkResult],
    ),
  );
  const rescueHeaderProgress =
    fixProgress
    ?? checkProgress
    ?? statusProgress;
  const rescueHeaderProgressValue = rescueHeaderProgress
    ? (rescueHeaderProgress.index + 1) / rescueHeaderProgress.total
    : getIdleRescueProgress(visibleRuntimeState);

  const primaryRecoveryVisible = shouldShowPrimaryRecovery(rescueState.runtimeState);
  const iconOnlyPrimaryAction = isIconOnlyPrimaryRescueAction(rescueState.runtimeState);
  const primaryActionIcon = getPrimaryRescueActionIcon(rescueState.runtimeState);
  const actionsDisabled =
    rescueState.statusChecking
    || rescueState.pendingAction !== null
    || (isRemote && !isConnected);

  return (
    <section>
      <h2 className="mb-4 text-2xl font-bold">{t("doctor.title")}</h2>
      <Card className="mb-4 gap-2 py-4">
        <CardHeader className="pb-0">
          <div className="flex flex-col items-center gap-3 text-center">
            <RescueAsciiHeader
              state={visibleRuntimeState}
              title={statusLabel}
              progress={rescueHeaderProgressValue}
              animateProgress={Boolean(rescueHeaderProgress)}
            />
            <div className="flex items-center justify-center gap-2">
              {iconOnlyPrimaryAction ? (
                <Button
                  variant="ghost"
                  size="icon-sm"
                  onClick={() => void runRescueAction(primaryAction)}
                  disabled={actionsDisabled}
                  aria-label={rescueState.pendingAction === primaryAction ? primaryActionBusyLabel : primaryActionLabel}
                  title={rescueState.pendingAction === primaryAction ? primaryActionBusyLabel : primaryActionLabel}
                  className={
                    primaryAction === "deactivate"
                      ? "text-muted-foreground hover:bg-destructive/10 hover:text-destructive transition-colors"
                      : "text-muted-foreground hover:bg-emerald-500/10 hover:text-emerald-700 dark:hover:text-emerald-300 transition-colors"
                  }
                >
                  {rescueState.pendingAction === primaryAction || rescueState.statusChecking ? (
                    <LoaderCircleIcon className="size-3.5 animate-spin" />
                  ) : primaryActionIcon === "pause" ? (
                    <PauseIcon className="size-3.5" />
                  ) : (
                    <PlayIcon className="size-3.5" />
                  )}
                </Button>
              ) : null}
              <Popover>
                <PopoverTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    disabled={actionsDisabled}
                    aria-label={t("doctor.rescueBotMore", {
                      defaultValue: "More options",
                    })}
                    title={t("doctor.rescueBotMore", {
                      defaultValue: "More options",
                    })}
                  >
                    <MoreHorizontalIcon className="size-3.5" />
                  </Button>
                </PopoverTrigger>
                <PopoverContent align="end" className="w-56 p-3">
                  <div className="grid gap-2">
                    <Button
                      variant="ghost"
                      size="sm"
                      className="justify-start"
                      onClick={() => {
                        void refreshRescueStatus();
                      }}
                      disabled={actionsDisabled}
                    >
                      <RefreshCwIcon className="size-3.5" />
                      {t("doctor.refresh", { defaultValue: "Check status" })}
                    </Button>
                    {rescueState.configured ? (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="justify-start text-muted-foreground hover:text-destructive"
                        onClick={() => void runRescueAction("unset")}
                        disabled={actionsDisabled}
                      >
                        <Trash2Icon className="size-3.5" />
                        {t("doctor.unset", { defaultValue: "Remove setup" })}
                      </Button>
                    ) : null}
                  </div>
                </PopoverContent>
              </Popover>
              <Button
                variant="ghost"
                size="icon-sm"
                onClick={() => openLogs("gateway")}
                aria-label={t("doctor.openLogs", { defaultValue: "Open logs" })}
                title={t("doctor.openLogs", { defaultValue: "Open logs" })}
                className="text-muted-foreground hover:text-foreground"
              >
                <FileTextIcon className="size-3.5" />
              </Button>
            </div>
            <div className="max-w-md text-sm text-muted-foreground">
              {t("doctor.rescueBotHint", {
                defaultValue:
                  "Safe checks and guided fixes before touching your main gateway.",
              })}
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {rescueState.error ? (
            <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              <div>{rescueState.error}</div>
              <div className="mt-2">
                <Button variant="outline" size="sm" onClick={() => openLogs("gateway")}>
                  {t("doctor.viewGatewayLogs", {
                    defaultValue: "View Gateway Logs",
                  })}
                </Button>
              </div>
            </div>
          ) : null}

          {primaryRecoveryVisible ? (
            <div className="border-t border-border/50 pt-4">
              <div className="flex items-center justify-between gap-3 flex-wrap">
                <div>
                  <h3 className="text-sm font-medium text-foreground/90">
                    {t("doctor.primaryRecoveryTitle", {
                      defaultValue: "Check Primary Agent",
                    })}
                  </h3>
                  <p className="mt-1 text-sm text-muted-foreground">
                    {t("doctor.primaryRecoveryHint", {
                      defaultValue:
                        "Run a structured recovery check across gateway, models, tools, agents, and channels.",
                    })}
                  </p>
                </div>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  onClick={handleCheckPrimaryViaRescue}
                  disabled={primaryState.checkLoading || primaryState.repairing || (isRemote && !isConnected)}
                  aria-label={t("doctor.primaryCheckNow", { defaultValue: "Check Primary Agent" })}
                  title={t("doctor.primaryCheckNow", { defaultValue: "Check Primary Agent" })}
                  className="text-muted-foreground hover:text-foreground"
                >
                  {primaryState.checkLoading
                    ? <LoaderCircleIcon className="size-3.5 animate-spin" />
                    : <StethoscopeIcon className="size-3.5" />}
                </Button>
              </div>

              {primaryState.checkLoading && checkProgress?.line ? (
                <div className="mt-4 h-5 overflow-hidden text-sm text-muted-foreground">
                  <span
                    key={checkProgress.line}
                    className="inline-block whitespace-nowrap transition-opacity duration-300 animate-pulse"
                  >
                    {checkProgress.line}
                  </span>
                </div>
              ) : null}

              {primaryState.checkError ? (
                <div className="mt-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                  <div>{primaryState.checkError}</div>
                  <div className="mt-2">
                    <Button variant="outline" size="sm" onClick={() => openLogs("gateway")}>
                      {t("doctor.viewGatewayLogs", {
                        defaultValue: "View Gateway Logs",
                      })}
                    </Button>
                  </div>
                </div>
              ) : null}

              {primaryState.checkResult ? (
                <DoctorRecoveryOverview
                  diagnosis={primaryState.checkResult}
                  checkLoading={primaryState.checkLoading}
                  repairing={primaryState.repairing}
                  progressLine={fixProgress?.line ?? null}
                  repairResult={primaryState.repairResult}
                  repairError={primaryState.repairError}
                  onRepairAll={() => void handleRepairPrimaryViaRescue()}
                  onRepairIssue={(issueId) => void handleRepairPrimaryViaRescue([issueId])}
                />
              ) : null}
            </div>
          ) : (
            <div className="rounded-md border border-border/50 bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
              {t("doctor.primaryRecoveryActivateHint", {
                defaultValue: "Enable the helper to unlock the primary recovery check.",
              })}
            </div>
          )}
        </CardContent>
      </Card>

      <DoctorLogsDialog
        open={logsOpen}
        onOpenChange={setLogsOpen}
        source={logsSource}
        onSourceChange={setLogsSource}
      />
    </section>
  );
}
