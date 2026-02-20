import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { api } from "@/lib/api";
import { useInstance } from "@/lib/instance-context";
import { cn } from "@/lib/utils";
import type {
  CronJob,
  CronRun,
  CronSchedule,
  WatchdogStatus,
  WatchdogJobStatus,
} from "@/lib/types";
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

/* ------------------------------------------------------------------ */
/*  Helper functions                                                   */
/* ------------------------------------------------------------------ */

function formatSchedule(schedule: CronSchedule, t: TFunction): string {
  if (schedule.kind === "every" && schedule.everyMs) {
    const mins = Math.round(schedule.everyMs / 60000);
    if (mins >= 60)
      return t("cron.every", { interval: `${Math.round(mins / 60)}h` });
    return t("cron.every", { interval: `${mins}m` });
  }
  if (schedule.kind === "at" && schedule.at) {
    return t("cron.oneShot", {
      time: new Date(schedule.at).toLocaleString(),
    });
  }
  if (schedule.kind === "cron" && schedule.expr) {
    return schedule.expr;
  }
  return "\u2014";
}

function formatRelativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const secs = Math.floor(diff / 1000);
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  return `${hours}h ago`;
}

function formatDuration(start: string, end: string): string {
  const ms = new Date(end).getTime() - new Date(start).getTime();
  if (ms < 1000) return `${ms}ms`;
  const secs = Math.round(ms / 1000);
  if (secs < 60) return `${secs}s`;
  return `${Math.floor(secs / 60)}m ${secs % 60}s`;
}

/* ------------------------------------------------------------------ */
/*  StatusBadge                                                        */
/* ------------------------------------------------------------------ */

function StatusBadge({ status }: { status: WatchdogJobStatus | undefined }) {
  const { t } = useTranslation();
  if (!status) return null;
  const colors: Record<string, string> = {
    ok: "bg-green-500/10 text-green-500",
    pending: "bg-muted text-muted-foreground",
    triggered: "bg-blue-500/10 text-blue-500",
    retrying: "bg-yellow-500/10 text-yellow-500",
    escalated: "bg-red-500/10 text-red-500",
  };
  return (
    <Badge
      variant="outline"
      className={cn("text-xs", colors[status] || "")}
    >
      {t(`cron.status.${status}`)}
    </Badge>
  );
}

/* ------------------------------------------------------------------ */
/*  Cron page                                                          */
/* ------------------------------------------------------------------ */

export function Cron() {
  const { t } = useTranslation();
  const { instanceId, isRemote, isConnected } = useInstance();

  // ---- state ----
  const [jobs, setJobs] = useState<CronJob[]>([]);
  const [watchdog, setWatchdog] = useState<
    (WatchdogStatus & { alive: boolean; deployed: boolean }) | null
  >(null);
  const [expandedJob, setExpandedJob] = useState<string | null>(null);
  const [runs, setRuns] = useState<Record<string, CronRun[]>>({});
  const [triggering, setTriggering] = useState<string | null>(null);
  const [wdAction, setWdAction] = useState<string | null>(null); // "deploying" | "starting" | "stopping"

  // ---- data loading ----

  const loadJobs = () => {
    const p = isRemote
      ? api.remoteListCronJobs(instanceId)
      : api.listCronJobs();
    p.then(setJobs).catch(() => {});
  };

  const loadWatchdog = () => {
    const p = isRemote
      ? api.remoteGetWatchdogStatus(instanceId)
      : api.getWatchdogStatus();
    p.then(setWatchdog).catch(() => setWatchdog(null));
  };

  const loadRuns = (jobId: string) => {
    const p = isRemote
      ? api.remoteGetCronRuns(instanceId, jobId, 10)
      : api.getCronRuns(jobId, 10);
    p.then((r) => setRuns((prev) => ({ ...prev, [jobId]: r }))).catch(
      () => {},
    );
  };

  useEffect(() => {
    loadJobs();
    loadWatchdog();
    const interval = setInterval(() => {
      loadJobs();
      loadWatchdog();
    }, 10_000);
    return () => clearInterval(interval);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId, isRemote]);

  // Load runs when a job is expanded
  useEffect(() => {
    if (expandedJob) loadRuns(expandedJob);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [expandedJob]);

  // ---- event handlers ----

  const handleTrigger = async (jobId: string) => {
    setTriggering(jobId);
    try {
      if (isRemote) {
        await api.remoteTriggerCronJob(instanceId, jobId);
      } else {
        await api.triggerCronJob(jobId);
      }
      loadJobs();
    } catch (err: any) {
      console.error(err);
    } finally {
      setTriggering(null);
    }
  };

  const handleDeploy = async () => {
    setWdAction("deploying");
    try {
      if (isRemote) {
        await api.remoteDeployWatchdog(instanceId, ""); // TODO: pass script content
      } else {
        await api.deployWatchdog();
      }
      loadWatchdog();
    } catch (err: any) {
      console.error(err);
    } finally {
      setWdAction(null);
    }
  };

  const handleStart = async () => {
    setWdAction("starting");
    try {
      if (isRemote) {
        await api.remoteStartWatchdog(instanceId);
      } else {
        await api.startWatchdog();
      }
      loadWatchdog();
    } catch (err: any) {
      console.error(err);
    } finally {
      setWdAction(null);
    }
  };

  const handleStop = async () => {
    setWdAction("stopping");
    try {
      if (isRemote) {
        await api.remoteStopWatchdog(instanceId);
      } else {
        await api.stopWatchdog();
      }
      loadWatchdog();
    } catch (err: any) {
      console.error(err);
    } finally {
      setWdAction(null);
    }
  };

  // ---- watchdog status logic ----

  let statusColor = "bg-gray-400";
  let statusText = t("watchdog.notDeployed");
  if (watchdog?.deployed && !watchdog?.alive) {
    statusColor = "bg-gray-400";
    statusText = t("watchdog.stopped");
  } else if (watchdog?.alive && watchdog?.lastCheckAt) {
    const lastCheckAge =
      Date.now() - new Date(watchdog.lastCheckAt).getTime();
    if (lastCheckAge <= 120_000) {
      statusColor = "bg-green-500";
      statusText = t("watchdog.running");
    } else {
      statusColor = "bg-red-500";
      statusText = t("watchdog.crashed");
    }
  }

  // ---- helpers for job rows ----

  const toggleExpand = (jobId: string) => {
    setExpandedJob((prev) => (prev === jobId ? null : jobId));
  };

  const getMonitorStatus = (
    job: CronJob,
  ): WatchdogJobStatus | undefined => {
    if (!watchdog?.jobs) return undefined;
    return watchdog.jobs[job.jobId]?.status;
  };

  const getLastRunText = (job: CronJob): string => {
    const jobRuns = runs[job.jobId];
    if (!jobRuns || jobRuns.length === 0) {
      const state = watchdog?.jobs?.[job.jobId];
      if (state?.lastRunAt) return formatRelativeTime(state.lastRunAt);
      return "\u2014";
    }
    return formatRelativeTime(jobRuns[0].startedAt);
  };

  // ---- render ----

  return (
    <section>
      <h2 className="text-2xl font-bold mb-4">{t("cron.pageTitle")}</h2>

      {/* Watchdog Control Bar */}
      <Card className="mb-4">
        <CardContent className="flex items-center gap-4 py-3">
          {/* Status indicator */}
          <div className="flex items-center gap-2">
            <span className={cn("w-2.5 h-2.5 rounded-full", statusColor)} />
            <span className="text-sm font-medium">{statusText}</span>
          </div>

          {/* Last check time */}
          {watchdog?.lastCheckAt && (
            <span className="text-sm text-muted-foreground">
              {t("watchdog.lastCheck", {
                time: formatRelativeTime(watchdog.lastCheckAt),
              })}
            </span>
          )}

          {/* Gateway health */}
          {watchdog?.alive && (
            <Badge
              variant={
                watchdog.gatewayHealthy ? "default" : "destructive"
              }
            >
              {t("watchdog.gateway", {
                status: watchdog.gatewayHealthy
                  ? "healthy"
                  : "unhealthy",
              })}
            </Badge>
          )}

          {/* Spacer */}
          <div className="flex-1" />

          {/* Action buttons */}
          {!watchdog?.deployed && (
            <Button size="sm" disabled={!!wdAction} onClick={handleDeploy}>
              {wdAction === "deploying"
                ? t("watchdog.deploying")
                : t("watchdog.deploy")}
            </Button>
          )}
          {watchdog?.deployed && !watchdog?.alive && (
            <Button size="sm" disabled={!!wdAction} onClick={handleStart}>
              {wdAction === "starting"
                ? t("watchdog.starting")
                : t("watchdog.start")}
            </Button>
          )}
          {watchdog?.alive && (
            <Button
              size="sm"
              variant="outline"
              disabled={!!wdAction}
              onClick={handleStop}
            >
              {wdAction === "stopping"
                ? t("watchdog.stopping")
                : t("watchdog.stop")}
            </Button>
          )}
        </CardContent>
      </Card>

      {/* Job Table */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("cron.title")}</CardTitle>
        </CardHeader>
        <CardContent>
          {jobs.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground">
              <p>{t("cron.noJobs")}</p>
              <p className="text-sm mt-1">{t("cron.noJobsHint")}</p>
            </div>
          ) : (
            <div className="space-y-1">
              {/* Header row */}
              <div className="grid grid-cols-[1fr_1fr_80px_120px_100px_80px] gap-2 px-3 py-1.5 text-xs text-muted-foreground font-medium">
                <span>{t("cron.name")}</span>
                <span>{t("cron.schedule")}</span>
                <span>{t("cron.agent")}</span>
                <span>{t("cron.lastRun")}</span>
                <span>{t("cron.monitor")}</span>
                <span>{t("cron.actions")}</span>
              </div>

              {/* Job rows */}
              {jobs.map((job) => (
                <div key={job.jobId}>
                  <div
                    className="grid grid-cols-[1fr_1fr_80px_120px_100px_80px] gap-2 px-3 py-2 rounded hover:bg-muted/50 cursor-pointer items-center"
                    onClick={() => toggleExpand(job.jobId)}
                  >
                    <span className="text-sm font-medium truncate">
                      {job.name}
                      {!job.enabled && (
                        <Badge
                          variant="outline"
                          className="ml-2 text-xs"
                        >
                          {t("cron.disabled")}
                        </Badge>
                      )}
                    </span>
                    <span className="text-sm text-muted-foreground">
                      {formatSchedule(job.schedule, t)}
                    </span>
                    <span className="text-sm text-muted-foreground">
                      {job.agentId || "main"}
                    </span>
                    <span className="text-sm text-muted-foreground">
                      {getLastRunText(job)}
                    </span>
                    <StatusBadge status={getMonitorStatus(job)} />
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={!!triggering}
                      onClick={(e) => {
                        e.stopPropagation();
                        handleTrigger(job.jobId);
                      }}
                    >
                      {triggering === job.jobId
                        ? t("cron.triggering")
                        : t("cron.trigger")}
                    </Button>
                  </div>

                  {/* Expanded run history */}
                  {expandedJob === job.jobId && (
                    <div className="px-6 py-2 mb-2">
                      <p className="text-xs font-medium text-muted-foreground mb-2">
                        {t("cron.runHistory")}
                      </p>
                      {(runs[job.jobId] || []).length === 0 ? (
                        <p className="text-xs text-muted-foreground">
                          {t("cron.noRuns")}
                        </p>
                      ) : (
                        <div className="space-y-1">
                          {(runs[job.jobId] || []).map((run, i) => (
                            <div
                              key={i}
                              className="grid grid-cols-[120px_80px_80px_1fr] gap-2 text-xs"
                            >
                              <span>
                                {new Date(
                                  run.startedAt,
                                ).toLocaleString()}
                              </span>
                              <Badge
                                variant={
                                  run.outcome === "ok"
                                    ? "default"
                                    : "destructive"
                                }
                                className="text-xs w-fit"
                              >
                                {run.outcome}
                              </Badge>
                              <span className="text-muted-foreground">
                                {run.endedAt
                                  ? formatDuration(
                                      run.startedAt,
                                      run.endedAt,
                                    )
                                  : "\u2014"}
                              </span>
                              {run.error && (
                                <span className="text-destructive truncate">
                                  {run.error}
                                </span>
                              )}
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </section>
  );
}
