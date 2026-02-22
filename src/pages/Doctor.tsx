import { useEffect, useReducer, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "@/lib/use-api";
import { useInstance } from "@/lib/instance-context";
import { useDoctorAgent } from "@/lib/use-doctor-agent";
import { initialState, reducer } from "@/lib/state";
import type { DoctorReport } from "@/lib/types";
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
import { SessionAnalysisPanel } from "@/components/SessionAnalysisPanel";
import { DoctorChat } from "@/components/DoctorChat";

type AgentSource = "local" | "hosted";

export function Doctor() {
  const { t } = useTranslation();
  const ua = useApi();
  const { instanceId, isRemote, isConnected } = useInstance();
  const doctor = useDoctorAgent();
  const [state, dispatch] = useReducer(reducer, initialState);
  const [rawOutput, setRawOutput] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [lastRefreshed, setLastRefreshed] = useState<string | null>(null);

  // Agent source state
  const [agentSource, setAgentSource] = useState<AgentSource>("hosted");
  const [diagnosing, setDiagnosing] = useState(false);

  // Reset doctor agent when switching instances
  useEffect(() => {
    doctor.reset();
    doctor.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId]);

  // Auto-infer target from active instance tab
  useEffect(() => {
    if (isRemote && isConnected) {
      doctor.setTarget(instanceId);
    } else {
      doctor.setTarget("local");
    }
  }, [instanceId, isRemote, isConnected, doctor.setTarget]);

  const handleStartDiagnosis = async () => {
    setDiagnosing(true);
    try {
      let url: string;
      if (agentSource === "local") {
        url = "ws://localhost:18789";
      } else {
        url = "wss://doctor.openclaw.ai";
      }

      await doctor.connect(url);

      // Collect context based on target (not source)
      const context = doctor.target === "local"
        ? await ua.collectDoctorContext()
        : await ua.collectDoctorContextRemote(doctor.target);

      await doctor.startDiagnosis(context);
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

  // Logs state
  const [logsOpen, setLogsOpen] = useState(false);
  const [logsSource, setLogsSource] = useState<"clawpal" | "gateway">("clawpal");
  const [logsTab, setLogsTab] = useState<"app" | "error">("app");
  const [logsContent, setLogsContent] = useState("");
  const [logsLoading, setLogsLoading] = useState(false);
  const logsContentRef = useRef<HTMLPreElement>(null);

  const hasReport = Boolean(state.doctor);
  const autoFixable = hasReport
    ? state.doctor!.issues
        .filter((issue) => issue.autoFixable)
        .map((issue) => issue.id)
    : [];

  function runDoctorCmd(): Promise<DoctorReport> {
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

  useEffect(() => {
    if (logsOpen) fetchLog(logsSource, logsTab);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [logsOpen, logsSource, logsTab]);

  useEffect(() => {
    // Reset state when switching instances
    dispatch({ type: "setDoctor", doctor: { ok: true, score: 0, issues: [] } as any });
    dispatch({ type: "setMessage", message: "" });
    setRawOutput(null);
    if (!ua.isRemote || ua.isConnected) {
      runDoctorCmd()
        .then((report) => dispatch({ type: "setDoctor", doctor: report }))
        .catch(() =>
          dispatch({ type: "setMessage", message: t('doctor.failedRunDoctor') }),
        );
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ua.instanceId, ua.isRemote, ua.isConnected]);

  return (
    <section>
      <h2 className="text-2xl font-bold mb-4">{t('doctor.title')}</h2>

      <div className="grid grid-cols-2 gap-3 mb-6">
        {/* Health Card */}
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

        {/* Logs Card */}
        <Card>
          <CardHeader>
            <CardTitle>{t("doctor.logs")}</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground mb-3">{t("doctor.logsDescription")}</p>
            <div className="flex gap-2">
              <Button variant="outline" size="sm" onClick={() => openLogs("clawpal")}>
                {t("doctor.clawpalLogs")}
              </Button>
              <Button variant="outline" size="sm" onClick={() => openLogs("gateway")}>
                {t("doctor.gatewayLogs")}
              </Button>
            </div>
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
          </CardContent>
        </Card>
      </div>

      {/* Doctor Agent Section */}
      <Card className="mb-6">
        <CardHeader>
          <CardTitle>{t("doctor.agentSource")}</CardTitle>
        </CardHeader>
        <CardContent>
          {!doctor.connected ? (
            <>
              {/* Target display */}
              <div className="flex items-center gap-2 mb-3 text-sm">
                <span className="text-muted-foreground">{t("doctor.target")}:</span>
                <span className="font-medium">
                  {doctor.target === "local" ? t("doctor.localMachine") : doctor.target}
                </span>
              </div>
              <div className="flex items-center gap-4 mb-4">
                <label className="flex items-center gap-1.5 text-sm cursor-pointer">
                  <input
                    type="radio"
                    name="agentSource"
                    value="local"
                    checked={agentSource === "local"}
                    onChange={() => setAgentSource("local")}
                    className="accent-primary"
                  />
                  {t("doctor.localGateway")}
                </label>
                <label className="flex items-center gap-1.5 text-sm cursor-pointer">
                  <input
                    type="radio"
                    name="agentSource"
                    value="hosted"
                    checked={agentSource === "hosted"}
                    onChange={() => setAgentSource("hosted")}
                    className="accent-primary"
                  />
                  {t("doctor.hostedService")}
                </label>
              </div>
              {doctor.error && (
                <div className="mb-3 text-sm text-destructive">{doctor.error}</div>
              )}
              <Button
                onClick={handleStartDiagnosis}
                disabled={diagnosing}
              >
                {diagnosing ? t("doctor.connecting") : t("doctor.startDiagnosis")}
              </Button>
            </>
          ) : (
            <>
              <div className="flex items-center justify-between mb-3">
                <Badge variant="outline" className="text-xs">
                  {agentSource === "local" ? t("doctor.localGateway")
                    : t("doctor.hostedService")}
                </Badge>
                <Badge variant={doctor.bridgeConnected ? "outline" : "destructive"} className="text-xs">
                  {doctor.bridgeConnected ? t("doctor.bridgeConnected") : t("doctor.bridgeDisconnected")}
                </Badge>
                <Button variant="outline" size="sm" onClick={handleStopDiagnosis}>
                  {t("doctor.stopDiagnosis")}
                </Button>
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

      <SessionAnalysisPanel />
    </section>
  );
}
