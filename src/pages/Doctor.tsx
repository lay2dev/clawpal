import { useEffect, useMemo, useReducer, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "@/lib/use-api";
import { initialState, reducer } from "@/lib/state";
import { formatBytes } from "@/lib/utils";
import type { AgentSessionAnalysis, SessionFile } from "@/lib/types";
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";

export function Doctor() {
  const { t } = useTranslation();
  const ua = useApi();
  const [state, dispatch] = useReducer(reducer, initialState);
  const [rawOutput, setRawOutput] = useState<string | null>(null);
  const [sessionFiles, setSessionFiles] = useState<SessionFile[]>([]);
  const [dataMessage, setDataMessage] = useState("");
  const [refreshing, setRefreshing] = useState(false);
  const [lastRefreshed, setLastRefreshed] = useState<string | null>(null);
  const [sessionAnalysis, setSessionAnalysis] = useState<AgentSessionAnalysis[] | null>(null);
  const [analyzing, setAnalyzing] = useState(false);
  const [expandedAgents, setExpandedAgents] = useState<Set<string>>(new Set());
  const [selectedSessions, setSelectedSessions] = useState<Map<string, Set<string>>>(new Map());
  const [deletingCategory, setDeletingCategory] = useState<{ agent: string; category: string } | null>(null);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [previewMessages, setPreviewMessages] = useState<{ role: string; content: string }[]>([]);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewTitle, setPreviewTitle] = useState("");

  // Logs state
  const [logsOpen, setLogsOpen] = useState(false);
  const [logsTab, setLogsTab] = useState<"app" | "error">("app");
  const [logsContent, setLogsContent] = useState("");
  const [logsLoading, setLogsLoading] = useState(false);
  const logsContentRef = useRef<HTMLPreElement>(null);

  const fetchLog = (which: "app" | "error") => {
    setLogsLoading(true);
    const fn = which === "app" ? ua.readAppLog : ua.readErrorLog;
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

  useEffect(() => {
    if (logsOpen) fetchLog(logsTab);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [logsOpen, logsTab]);

  const hasReport = Boolean(state.doctor);
  const autoFixable = hasReport
    ? state.doctor!.issues
        .filter((issue) => issue.autoFixable)
        .map((issue) => issue.id)
    : [];

  const agents = useMemo(() => {
    const map = new Map<string, { count: number; size: number }>();
    for (const f of sessionFiles) {
      const entry = map.get(f.agent) || { count: 0, size: 0 };
      entry.count += 1;
      entry.size += f.sizeBytes;
      map.set(f.agent, entry);
    }
    return Array.from(map.entries()).map(([agent, info]) => ({
      agent,
      count: info.count,
      size: info.size,
    }));
  }, [sessionFiles]);

  const totalSessionBytes = useMemo(
    () => sessionFiles.reduce((sum, f) => sum + f.sizeBytes, 0),
    [sessionFiles],
  );

  function runDoctorCmd(): Promise<import("@/lib/types").DoctorReport> {
    return ua.runDoctor().then((report) => {
      const raw = (report as any).rawOutput;
      if (raw && typeof raw === "string") setRawOutput(raw);
      else setRawOutput(null);
      return report;
    });
  }

  function fixIssuesCmd(ids: string[]) {
    return ua.fixIssues(ids);
  }

  function refreshData() {
    ua.listSessionFiles()
      .then(setSessionFiles)
      .catch(() => setDataMessage(t('doctor.failedLoadSessions')));
  }

  function removeSessionsFromAnalysis(agent: string, deletedIds: Set<string>) {
    setSessionAnalysis((prev) => {
      if (!prev) return prev;
      return prev
        .map((a) => {
          if (a.agent !== agent) return a;
          const remaining = a.sessions.filter((s) => !deletedIds.has(s.sessionId));
          return {
            ...a,
            sessions: remaining,
            totalFiles: remaining.length,
            totalSizeBytes: remaining.reduce((sum, s) => sum + s.sizeBytes, 0),
            emptyCount: remaining.filter((s) => s.category === "empty").length,
            lowValueCount: remaining.filter((s) => s.category === "low_value").length,
            valuableCount: remaining.filter((s) => s.category === "valuable").length,
          };
        })
        .filter((a) => a.totalFiles > 0);
    });
  }

  useEffect(() => {
    if (!ua.isRemote) {
      runDoctorCmd()
        .then((report) => dispatch({ type: "setDoctor", doctor: report }))
        .catch(() =>
          dispatch({ type: "setMessage", message: t('doctor.failedRunDoctor') }),
        );
    }
    refreshData();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ua.instanceId, ua.isRemote, ua.isConnected]);


  return (
    <section>
      <h2 className="text-2xl font-bold mb-4">{t('doctor.title')}</h2>

      <div className={`grid ${ua.isRemote ? '' : 'grid-cols-2'} gap-3 mb-6`}>
        {/* Health Card — local only */}
        {!ua.isRemote && (
          <Card>
            <CardHeader>
              <CardTitle>{t("doctor.health")}</CardTitle>
            </CardHeader>
            <CardContent>
              {state.doctor && (
                <>
                  <p className="text-sm text-muted-foreground mb-3">
                    {t('doctor.healthScore', { score: state.doctor.score })}
                  </p>
                  <div className="space-y-2">
                    {state.doctor.issues.map((issue) => (
                      <div
                        key={issue.id}
                        className="flex items-center gap-2 text-sm"
                      >
                        {issue.severity === "error" && (
                          <Badge variant="destructive">ERROR</Badge>
                        )}
                        {issue.severity === "warn" && (
                          <Badge variant="secondary">WARN</Badge>
                        )}
                        {issue.severity === "info" && (
                          <Badge variant="outline">INFO</Badge>
                        )}
                        <span>{issue.message}</span>
                        {issue.autoFixable && (
                          <Button
                            size="sm"
                            onClick={() => {
                              fixIssuesCmd([issue.id])
                                .then(() => runDoctorCmd())
                                .then((report) =>
                                  dispatch({ type: "setDoctor", doctor: report }),
                                )
                                .catch(() =>
                                  dispatch({
                                    type: "setMessage",
                                    message: t('doctor.failedFix'),
                                  }),
                                );
                            }}
                          >
                            {t('doctor.fix')}
                          </Button>
                        )}
                      </div>
                    ))}
                  </div>
                  <div className="flex gap-2 mt-3">
                    <Button
                      variant="outline"
                      onClick={() => {
                        fixIssuesCmd(autoFixable)
                          .then(() => runDoctorCmd())
                          .then((report) =>
                            dispatch({ type: "setDoctor", doctor: report }),
                          )
                          .catch(() =>
                            dispatch({
                              type: "setMessage",
                              message: t('doctor.failedFixAll'),
                            }),
                          );
                      }}
                      disabled={!autoFixable.length}
                    >
                      {t('doctor.fixAll')}
                    </Button>
                    <Button
                      variant="outline"
                      disabled={refreshing}
                      onClick={() => {
                        setRefreshing(true);
                        runDoctorCmd()
                          .then((report) => {
                            dispatch({ type: "setDoctor", doctor: report });
                            setLastRefreshed(new Date().toLocaleTimeString());
                            refreshData();
                          })
                          .catch(() =>
                            dispatch({
                              type: "setMessage",
                              message: t('doctor.refreshFailed'),
                            }),
                          )
                          .finally(() => setRefreshing(false));
                      }}
                    >
                      {refreshing ? t('doctor.refreshing') : t('doctor.refresh')}
                    </Button>
                    {lastRefreshed && (
                      <span className="text-xs text-muted-foreground ml-2">
                        {t('doctor.lastRefreshed', { time: lastRefreshed })}
                      </span>
                    )}
                  </div>
                </>
              )}
              {!hasReport && (
                <Button
                  onClick={() =>
                    runDoctorCmd()
                      .then((report) =>
                        dispatch({ type: "setDoctor", doctor: report }),
                      )
                  }
                >
                  {t('doctor.runDoctor')}
                </Button>
              )}
              {state.message && <p className="text-sm text-muted-foreground mt-2">{state.message}</p>}
            </CardContent>
          </Card>
        )}

        {/* Logs Card — always visible */}
        <Card>
          <CardHeader>
            <CardTitle>{t("doctor.logs")}</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground mb-3">{t("doctor.logsDescription")}</p>
            <Dialog open={logsOpen} onOpenChange={setLogsOpen}>
              <DialogTrigger asChild>
                <Button variant="outline" size="sm">{t("doctor.viewLogs")}</Button>
              </DialogTrigger>
              <DialogContent className="sm:max-w-2xl max-h-[80vh] flex flex-col">
                <DialogHeader>
                  <DialogTitle>{t("doctor.logs")}</DialogTitle>
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
                    onClick={() => fetchLog(logsTab)}
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
          </CardContent>
        </Card>
      </div>

      {/* Data Cleanup */}
      <h3 className="text-lg font-semibold mt-6 mb-3">
        {t('doctor.dataCleanup')}
      </h3>
      {dataMessage && (
        <p className="text-sm text-muted-foreground mt-2">{dataMessage}</p>
      )}

      <div className="space-y-3">
        {/* Sessions */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center justify-between">
              <span>{t('doctor.sessions')}</span>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  disabled={analyzing}
                  onClick={() => {
                    setAnalyzing(true);
                    ua.analyzeSessions()
                      .then((data) => {
                        setSessionAnalysis(data);
                        setExpandedAgents(new Set());
                        setSelectedSessions(new Map());
                      })
                      .catch(() => setDataMessage(t('doctor.failedAnalyze')))
                      .finally(() => setAnalyzing(false));
                  }}
                >
                  {analyzing ? t('doctor.analyzing') : t('doctor.analyze')}
                </Button>
                <AlertDialog>
                  <AlertDialogTrigger asChild>
                    <Button size="sm" variant="destructive" disabled={sessionFiles.length === 0}>
                      {t('doctor.clearAll')}
                    </Button>
                  </AlertDialogTrigger>
                  <AlertDialogContent>
                    <AlertDialogHeader>
                      <AlertDialogTitle>{t('doctor.clearAllTitle')}</AlertDialogTitle>
                      <AlertDialogDescription>
                        {t('doctor.clearAllDescription', { count: sessionFiles.length })}
                      </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                      <AlertDialogCancel>{t('config.cancel')}</AlertDialogCancel>
                      <AlertDialogAction
                        className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                        onClick={() => {
                          ua.clearAllSessions()
                            .then((count) => {
                              setDataMessage(t('doctor.clearedSessions', { count }));
                              setSessionAnalysis(null);
                              refreshData();
                            })
                            .catch(() => setDataMessage(t('doctor.failedClear')));
                        }}
                      >
                        {t('doctor.clear')}
                      </AlertDialogAction>
                    </AlertDialogFooter>
                  </AlertDialogContent>
                </AlertDialog>
              </div>
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground mb-3">
              {t('doctor.filesCount', { count: sessionFiles.length, size: formatBytes(totalSessionBytes) })}
            </p>

            {!sessionAnalysis ? (
              /* Basic agent list (before analysis) */
              <div className="space-y-1">
                {agents.map((a) => (
                  <div key={a.agent} className="text-sm">
                    {a.agent}: {a.count} files ({formatBytes(a.size)})
                  </div>
                ))}
              </div>
            ) : (
              /* Analysis results: two-level view */
              <div className="space-y-3">
                {sessionAnalysis.length === 0 && (
                  <p className="text-sm text-muted-foreground">{t('doctor.noSessionFiles')}</p>
                )}
                {sessionAnalysis.map((agentData) => {
                  const isExpanded = expandedAgents.has(agentData.agent);
                  const agentSelected = selectedSessions.get(agentData.agent) || new Set<string>();

                  const deleteSessionsFn = (ids: string[]) =>
                    ua.deleteSessionsByIds(agentData.agent, ids);

                  return (
                    <div key={agentData.agent} className="border rounded-md p-3">
                      {/* Agent summary row */}
                      <div className="flex items-center justify-between mb-2">
                        <div>
                          <span className="font-medium text-sm">{agentData.agent}</span>
                          <span className="text-xs text-muted-foreground ml-2">
                            {t('doctor.filesCount', { count: agentData.totalFiles, size: formatBytes(agentData.totalSizeBytes) })}
                          </span>
                        </div>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => {
                            setExpandedAgents((prev) => {
                              const next = new Set(prev);
                              if (next.has(agentData.agent)) next.delete(agentData.agent);
                              else next.add(agentData.agent);
                              return next;
                            });
                          }}
                        >
                          {isExpanded ? t('doctor.collapse') : t('doctor.details')}
                        </Button>
                      </div>

                      {/* Category badges */}
                      <div className="flex items-center gap-2 mb-2 flex-wrap">
                        {agentData.emptyCount > 0 && (
                          <Badge variant="destructive" className="text-xs">
                            {t('doctor.empty', { count: agentData.emptyCount })}
                          </Badge>
                        )}
                        {agentData.lowValueCount > 0 && (
                          <Badge variant="secondary" className="text-xs bg-yellow-500/15 text-yellow-700 dark:text-yellow-400">
                            {t('doctor.lowValue', { count: agentData.lowValueCount })}
                          </Badge>
                        )}
                        {agentData.valuableCount > 0 && (
                          <Badge variant="secondary" className="text-xs bg-green-500/15 text-green-700 dark:text-green-400">
                            {t('doctor.valuable', { count: agentData.valuableCount })}
                          </Badge>
                        )}
                      </div>

                      {/* Quick-clean buttons & batch actions */}
                      <div className="flex gap-2 flex-wrap">
                        {agentData.emptyCount > 0 && (
                          <AlertDialog
                            open={deletingCategory?.agent === agentData.agent && deletingCategory?.category === "empty"}
                            onOpenChange={(open) => !open && setDeletingCategory(null)}
                          >
                            <AlertDialogTrigger asChild>
                              <Button
                                size="sm"
                                variant="outline"
                                className="text-xs h-7"
                                onClick={() => setDeletingCategory({ agent: agentData.agent, category: "empty" })}
                              >
                                {t('doctor.cleanEmpty')}
                              </Button>
                            </AlertDialogTrigger>
                            <AlertDialogContent>
                              <AlertDialogHeader>
                                <AlertDialogTitle>{t('doctor.cleanEmptyTitle')}</AlertDialogTitle>
                                <AlertDialogDescription>
                                  {t('doctor.cleanEmptyDescription', { count: agentData.emptyCount, agent: agentData.agent })}
                                </AlertDialogDescription>
                              </AlertDialogHeader>
                              <AlertDialogFooter>
                                <AlertDialogCancel>{t('config.cancel')}</AlertDialogCancel>
                                <AlertDialogAction
                                  className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                                  onClick={() => {
                                    const ids = agentData.sessions
                                      .filter((s) => s.category === "empty")
                                      .map((s) => s.sessionId);
                                    deleteSessionsFn(ids)
                                      .then((count) => {
                                        setDataMessage(t('doctor.deletedEmpty', { count, agent: agentData.agent }));
                                        removeSessionsFromAnalysis(agentData.agent, new Set(ids));
                                        refreshData();
                                      })
                                      .catch(() => setDataMessage(t('doctor.failedDelete')));
                                  }}
                                >
                                  {t('home.delete')}
                                </AlertDialogAction>
                              </AlertDialogFooter>
                            </AlertDialogContent>
                          </AlertDialog>
                        )}
                        {agentData.lowValueCount > 0 && (
                          <AlertDialog
                            open={deletingCategory?.agent === agentData.agent && deletingCategory?.category === "low_value"}
                            onOpenChange={(open) => !open && setDeletingCategory(null)}
                          >
                            <AlertDialogTrigger asChild>
                              <Button
                                size="sm"
                                variant="outline"
                                className="text-xs h-7"
                                onClick={() => setDeletingCategory({ agent: agentData.agent, category: "low_value" })}
                              >
                                {t('doctor.cleanLowValue')}
                              </Button>
                            </AlertDialogTrigger>
                            <AlertDialogContent>
                              <AlertDialogHeader>
                                <AlertDialogTitle>{t('doctor.cleanLowValueTitle')}</AlertDialogTitle>
                                <AlertDialogDescription>
                                  {t('doctor.cleanLowValueDescription', { count: agentData.lowValueCount, agent: agentData.agent })}
                                </AlertDialogDescription>
                              </AlertDialogHeader>
                              <AlertDialogFooter>
                                <AlertDialogCancel>{t('config.cancel')}</AlertDialogCancel>
                                <AlertDialogAction
                                  className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                                  onClick={() => {
                                    const ids = agentData.sessions
                                      .filter((s) => s.category === "low_value")
                                      .map((s) => s.sessionId);
                                    deleteSessionsFn(ids)
                                      .then((count) => {
                                        setDataMessage(t('doctor.deletedLowValue', { count, agent: agentData.agent }));
                                        removeSessionsFromAnalysis(agentData.agent, new Set(ids));
                                        refreshData();
                                      })
                                      .catch(() => setDataMessage(t('doctor.failedDelete')));
                                  }}
                                >
                                  {t('home.delete')}
                                </AlertDialogAction>
                              </AlertDialogFooter>
                            </AlertDialogContent>
                          </AlertDialog>
                        )}
                        {agentSelected.size > 0 && (
                          <>
                            <AlertDialog>
                              <AlertDialogTrigger asChild>
                                <Button size="sm" variant="destructive" className="text-xs h-7">
                                  {t('doctor.deleteSelected', { count: agentSelected.size })}
                                </Button>
                              </AlertDialogTrigger>
                              <AlertDialogContent>
                                <AlertDialogHeader>
                                  <AlertDialogTitle>{t('doctor.deleteSelectedTitle')}</AlertDialogTitle>
                                  <AlertDialogDescription>
                                    {t('doctor.deleteSelectedDescription', { count: agentSelected.size, agent: agentData.agent })}
                                  </AlertDialogDescription>
                                </AlertDialogHeader>
                                <AlertDialogFooter>
                                  <AlertDialogCancel>{t('config.cancel')}</AlertDialogCancel>
                                  <AlertDialogAction
                                    className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                                    onClick={() => {
                                      const ids = Array.from(agentSelected);
                                      deleteSessionsFn(ids)
                                        .then((count) => {
                                          setDataMessage(t('doctor.deletedSelected', { count, agent: agentData.agent }));
                                          removeSessionsFromAnalysis(agentData.agent, new Set(ids));
                                          setSelectedSessions((prev) => {
                                            const next = new Map(prev);
                                            next.delete(agentData.agent);
                                            return next;
                                          });
                                          refreshData();
                                        })
                                        .catch(() => setDataMessage(t('doctor.failedDelete')));
                                    }}
                                  >
                                    {t('home.delete')}
                                  </AlertDialogAction>
                                </AlertDialogFooter>
                              </AlertDialogContent>
                            </AlertDialog>
                            <Button
                              size="sm"
                              variant="ghost"
                              className="text-xs h-7"
                              onClick={() => {
                                setSelectedSessions((prev) => {
                                  const next = new Map(prev);
                                  next.delete(agentData.agent);
                                  return next;
                                });
                              }}
                            >
                              {t('doctor.deselect')}
                            </Button>
                          </>
                        )}
                      </div>

                      {/* Expanded session details */}
                      {isExpanded && (
                        <div className="mt-3 space-y-1">
                          {agentData.sessions.map((session) => {
                            const isChecked = agentSelected.has(session.sessionId);
                            const categoryColor =
                              session.category === "empty"
                                ? "text-red-500"
                                : session.category === "low_value"
                                  ? "text-yellow-500"
                                  : "text-green-500";
                            const categoryDot =
                              session.category === "empty"
                                ? "bg-red-500"
                                : session.category === "low_value"
                                  ? "bg-yellow-500"
                                  : "bg-green-500";

                            const ageLabel = session.ageDays < 1
                              ? "< 1d"
                              : session.ageDays < 30
                                ? `${Math.round(session.ageDays)}d`
                                : `${Math.round(session.ageDays / 30)}mo`;

                            return (
                              <div
                                key={session.sessionId}
                                className="flex items-center gap-2 text-xs py-1 px-2 rounded hover:bg-muted/50"
                              >
                                <Checkbox
                                  checked={isChecked}
                                  onCheckedChange={(checked) => {
                                    setSelectedSessions((prev) => {
                                      const next = new Map(prev);
                                      const agentSet = new Set(next.get(agentData.agent) || []);
                                      if (checked) agentSet.add(session.sessionId);
                                      else agentSet.delete(session.sessionId);
                                      next.set(agentData.agent, agentSet);
                                      return next;
                                    });
                                  }}
                                />
                                <span className={`w-2 h-2 rounded-full shrink-0 ${categoryDot}`} />
                                <button
                                  className="font-mono w-20 truncate text-left underline decoration-dotted hover:text-foreground text-muted-foreground"
                                  title={`Preview ${session.sessionId}`}
                                  onClick={() => {
                                    setPreviewTitle(`${agentData.agent} / ${session.sessionId.slice(0, 12)}`);
                                    setPreviewMessages([]);
                                    setPreviewLoading(true);
                                    setPreviewOpen(true);
                                    ua.previewSession(agentData.agent, session.sessionId)
                                      .then(setPreviewMessages)
                                      .catch(() => setPreviewMessages([{ role: "error", content: t('doctor.failedLoadSession') }]))
                                      .finally(() => setPreviewLoading(false));
                                  }}
                                >
                                  {session.sessionId.slice(0, 8)}
                                </button>
                                <span className="w-16 text-right">{formatBytes(session.sizeBytes)}</span>
                                <span className="w-16 text-right">{t('doctor.msgs', { count: session.messageCount })}</span>
                                <span className="w-12 text-right text-muted-foreground">{ageLabel}</span>
                                <span className="w-16 truncate text-muted-foreground" title={session.model || ""}>
                                  {session.model || "—"}
                                </span>
                                <span className={`w-16 ${categoryColor}`}>
                                  {session.category === "low_value" ? "low" : session.category}
                                </span>
                              </div>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Session Preview Dialog */}
      <Dialog open={previewOpen} onOpenChange={setPreviewOpen}>
        <DialogContent className="max-w-2xl max-h-[70vh] flex flex-col">
          <DialogHeader>
            <DialogTitle className="font-mono text-sm">{previewTitle}</DialogTitle>
          </DialogHeader>
          <div className="flex-1 overflow-y-auto space-y-3 text-sm">
            {previewLoading && <p className="text-muted-foreground">{t('doctor.loading')}</p>}
            {!previewLoading && previewMessages.length === 0 && (
              <p className="text-muted-foreground">{t('doctor.noMessages')}</p>
            )}
            {previewMessages.map((msg, i) => (
              <div key={i} className={`rounded-md p-2 ${msg.role === "user" ? "bg-muted" : msg.role === "assistant" ? "bg-primary/5" : "bg-destructive/10"}`}>
                <div className="text-xs font-medium text-muted-foreground mb-1">{msg.role}</div>
                <div className="whitespace-pre-wrap break-words">{msg.content}</div>
              </div>
            ))}
          </div>
        </DialogContent>
      </Dialog>
    </section>
  );
}
