import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { useApi } from "@/lib/use-api";
import { useInstance } from "@/lib/instance-context";
import { useDoctorAgent } from "@/lib/use-doctor-agent";
import type { DoctorReport, SshHost } from "@/lib/types";
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

export function Doctor({ sshHosts }: DoctorProps) {
  const { t } = useTranslation();
  const ua = useApi();
  const { instanceId, isRemote, isConnected } = useInstance();
  const doctor = useDoctorAgent();

  // Simple state replacing useReducer
  const [report, setReport] = useState<DoctorReport | null>(null);
  const [message, setMessage] = useState("");
  const [refreshing, setRefreshing] = useState(false);

  // Agent source: an instance id ("local" / host uuid) or "remote" (hosted doctor)
  const [agentSource, setAgentSource] = useState("remote");
  const [diagnosing, setDiagnosing] = useState(false);

  // Logs state
  const [logsOpen, setLogsOpen] = useState(false);
  const [logsSource, setLogsSource] = useState<"clawpal" | "gateway">("clawpal");
  const [logsTab, setLogsTab] = useState<"app" | "error">("app");
  const [logsContent, setLogsContent] = useState("");
  const [logsLoading, setLogsLoading] = useState(false);
  const logsContentRef = useRef<HTMLPreElement>(null);

  // Reset doctor agent when switching instances
  useEffect(() => {
    doctor.reset();
    doctor.disconnect();
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

  function runDoctorCmd(): Promise<DoctorReport> {
    return ua.runDoctor().then((r) => r);
  }

  function fixIssuesCmd(ids: string[]) {
    return ua.fixIssues(ids);
  }

  // Load report on instance change
  useEffect(() => {
    setReport(null);
    setMessage("");
    if (!ua.isRemote || ua.isConnected) {
      runDoctorCmd()
        .then(setReport)
        .catch(() => setMessage(t("doctor.failedRunDoctor")));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ua.instanceId, ua.isRemote, ua.isConnected]);

  const handleStartDiagnosis = async () => {
    setDiagnosing(true);
    try {
      let url: string;
      if (agentSource === "remote") {
        url = "wss://doctor.openclaw.ai";
      } else if (agentSource === "local") {
        url = "ws://localhost:18789";
      } else {
        // Remote gateway: SSH tunnel to its port 18789
        const localPort = await api.doctorPortForward(agentSource);
        url = `ws://localhost:${localPort}`;
      }

      await doctor.connect(url);

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

  const handleRefresh = () => {
    setRefreshing(true);
    runDoctorCmd()
      .then(setReport)
      .catch(() => setMessage(t("doctor.refreshFailed")))
      .finally(() => setRefreshing(false));
  };

  const autoFixable = report
    ? report.issues.filter((i) => i.autoFixable).map((i) => i.id)
    : [];

  const issueCount = report ? report.issues.length : 0;

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

  useEffect(() => {
    if (logsOpen) fetchLog(logsSource, logsTab);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [logsOpen, logsSource, logsTab]);

  return (
    <section>
      <h2 className="text-2xl font-bold mb-4">{t("doctor.title")}</h2>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <CardTitle>{t("doctor.agentSource")}</CardTitle>
              {report && (
                <span className="text-sm text-muted-foreground">
                  {t("doctor.healthSummary", { score: report.score })}
                  {issueCount > 0 && (
                    <Badge variant="secondary" className="ml-1.5 text-xs">
                      {issueCount}
                    </Badge>
                  )}
                </span>
              )}
            </div>
            <div className="flex items-center gap-1">
              <Button variant="ghost" size="sm" onClick={() => openLogs("clawpal")}>
                {t("doctor.clawpalLogs")}
              </Button>
              <Button variant="ghost" size="sm" onClick={() => openLogs("gateway")}>
                {t("doctor.gatewayLogs")}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={handleRefresh}
                disabled={refreshing}
              >
                {refreshing ? t("doctor.refreshing") : t("doctor.refresh")}
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {/* Health issues (compact) */}
          {report && report.issues.length > 0 && (
            <div className="mb-4 space-y-1.5">
              {report.issues.map((issue) => (
                <div key={issue.id} className="flex items-center gap-2 text-sm">
                  {issue.severity === "error" && <Badge variant="destructive">ERROR</Badge>}
                  {issue.severity === "warn" && <Badge variant="secondary">WARN</Badge>}
                  {issue.severity === "info" && <Badge variant="outline">INFO</Badge>}
                  <span>{issue.message}</span>
                  {issue.autoFixable && (
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => {
                        fixIssuesCmd([issue.id])
                          .then(() => runDoctorCmd())
                          .then(setReport)
                          .catch(() => setMessage(t("doctor.failedFix")));
                      }}
                    >
                      {t("doctor.fix")}
                    </Button>
                  )}
                </div>
              ))}
              {autoFixable.length > 0 && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    fixIssuesCmd(autoFixable)
                      .then(() => runDoctorCmd())
                      .then(setReport)
                      .catch(() => setMessage(t("doctor.failedFixAll")));
                  }}
                >
                  {t("doctor.fixAll")}
                </Button>
              )}
            </div>
          )}

          {message && <p className="text-sm text-muted-foreground mb-3">{message}</p>}

          {!doctor.connected ? (
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
                <div className="mb-3 text-sm text-destructive">{doctor.error}</div>
              )}
              <Button onClick={handleStartDiagnosis} disabled={diagnosing}>
                {diagnosing ? t("doctor.connecting") : t("doctor.startDiagnosis")}
              </Button>
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
                  <Badge variant="outline" className="text-xs">
                    {doctor.bridgeConnected ? t("doctor.bridgeConnected") : t("doctor.bridgeDisconnected")}
                  </Badge>
                </div>
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
