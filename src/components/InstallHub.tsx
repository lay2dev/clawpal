import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { SshFormWidget } from "@/components/SshFormWidget";
import { hasGuidanceEmitted } from "@/lib/use-api";
import { isAlreadyExplainedGuidanceError, withGuidance } from "@/lib/guidance";
import { api } from "@/lib/api";
import { clearRemotePersistenceScope, ensureRemotePersistenceScope } from "@/lib/instance-persistence";
import type {
  InstallSession,
  SshConfigHostSuggestion,
  SshHost,
} from "@/lib/types";
import {
  buildDefaultSshHostId,
  sanitizeLocalIdSegment,
} from "./install-hub-helpers";

const DIAGNOSTIC_LOG_LINES = 300;

type InstallHubMode =
  | "idle"
  | "failed"
  | "connect_ssh"
  | "connect_docker"
  | "connect_wsl2";

type InstallHubDiagnosticLogs = {
  appLog: string;
  errorLog: string;
  gatewayLog: string;
  gatewayErrorLog: string;
};

const EMPTY_DIAGNOSTIC_LOGS: InstallHubDiagnosticLogs = {
  appLog: "",
  errorLog: "",
  gatewayLog: "",
  gatewayErrorLog: "",
};

export function InstallHubEntryCards({
  onConnectRemote,
  onConnectDocker,
  onConnectWsl2,
}: {
  onConnectRemote: () => void;
  onConnectDocker: () => void;
  onConnectWsl2: () => void;
}) {
  const { t } = useTranslation();

  return (
    <div className="grid gap-3">
      <Button
        variant="outline"
        className="h-auto w-full min-w-0 flex-col items-start justify-start gap-1 px-4 py-4 text-left whitespace-normal break-words"
        onClick={onConnectRemote}
      >
        <span className="w-full font-medium whitespace-normal break-words">
          {t("installChat.tag.connectRemote")}
        </span>
        <span className="w-full text-xs text-muted-foreground whitespace-normal break-words">
          {t("installChat.connectRemoteDescription")}
        </span>
      </Button>
      <Button
        variant="outline"
        className="h-auto w-full min-w-0 flex-col items-start justify-start gap-1 px-4 py-4 text-left whitespace-normal break-words"
        onClick={onConnectDocker}
      >
        <span className="w-full font-medium whitespace-normal break-words">
          {t("installChat.tag.connectDocker")}
        </span>
        <span className="w-full text-xs text-muted-foreground whitespace-normal break-words">
          {t("installChat.connectDockerDescription")}
        </span>
      </Button>
      <Button
        variant="outline"
        className="h-auto w-full min-w-0 flex-col items-start justify-start gap-1 px-4 py-4 text-left whitespace-normal break-words"
        onClick={onConnectWsl2}
      >
        <span className="w-full font-medium whitespace-normal break-words">
          {t("installChat.tag.connectWsl2")}
        </span>
        <span className="w-full text-xs text-muted-foreground whitespace-normal break-words">
          {t("installChat.connectWsl2Description")}
        </span>
      </Button>
    </div>
  );
}

export function InstallHub({
  open,
  onOpenChange,
  showToast,
  onNavigate: _onNavigate,
  onReady,
  onOpenDoctor,
  connectRemoteHost,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  showToast?: (message: string, type?: "success" | "error") => void;
  onNavigate?: (route: string) => void;
  onReady?: (session: InstallSession) => void;
  onOpenDoctor?: () => void;
  connectRemoteHost?: (hostId: string) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<InstallHubMode>("idle");
  const [connectSubmitting, setConnectSubmitting] = useState(false);
  const [dockerConnectHome, setDockerConnectHome] = useState("~/.clawpal/docker-local");
  const [dockerConnectLabel, setDockerConnectLabel] = useState("docker-local");
  const [wsl2ConnectHome, setWsl2ConnectHome] = useState("");
  const [wsl2ConnectLabel, setWsl2ConnectLabel] = useState("wsl2-default");
  const [sshConfigSuggestions, setSshConfigSuggestions] = useState<SshConfigHostSuggestion[]>([]);
  const [sshConfigSuggestionsLoading, setSshConfigSuggestionsLoading] = useState(false);
  const [sshConfigSuggestionsError, setSshConfigSuggestionsError] = useState<string | null>(null);
  const [sshConfigSuggestionsLoaded, setSshConfigSuggestionsLoaded] = useState(false);
  const [diagnosticHostId, setDiagnosticHostId] = useState<string | null>(null);
  const [diagnosticLogs, setDiagnosticLogs] = useState<InstallHubDiagnosticLogs>(EMPTY_DIAGNOSTIC_LOGS);
  const [diagnosticsLoading, setDiagnosticsLoading] = useState(false);
  const [diagnosticsVisible, setDiagnosticsVisible] = useState(false);
  const [diagnosticsError, setDiagnosticsError] = useState<string | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [runErrorHasGuidance, setRunErrorHasGuidance] = useState(false);

  useEffect(() => {
    if (!open) {
      setMode("idle");
      setConnectSubmitting(false);
      setDockerConnectHome("~/.clawpal/docker-local");
      setDockerConnectLabel("docker-local");
      setWsl2ConnectHome("");
      setWsl2ConnectLabel("wsl2-default");
      setSshConfigSuggestions([]);
      setSshConfigSuggestionsLoading(false);
      setSshConfigSuggestionsError(null);
      setSshConfigSuggestionsLoaded(false);
      setDiagnosticHostId(null);
      setDiagnosticLogs(EMPTY_DIAGNOSTIC_LOGS);
      setDiagnosticsLoading(false);
      setDiagnosticsVisible(false);
      setDiagnosticsError(null);
      setRunError(null);
      setRunErrorHasGuidance(false);
    }
  }, [open]);

  const loadSshConfigSuggestions = useCallback(async () => {
    if (sshConfigSuggestionsLoading || sshConfigSuggestionsLoaded) return;
    setSshConfigSuggestionsLoading(true);
    setSshConfigSuggestionsError(null);
    try {
      const list = await withGuidance(
        () => api.listSshConfigHosts(),
        "listSshConfigHosts",
        "local",
        "local",
      );
      setSshConfigSuggestions(list);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setSshConfigSuggestions([]);
      setSshConfigSuggestionsError(message);
      showToast?.(message, "error");
    } finally {
      setSshConfigSuggestionsLoaded(true);
      setSshConfigSuggestionsLoading(false);
    }
  }, [showToast, sshConfigSuggestionsLoaded, sshConfigSuggestionsLoading]);

  useEffect(() => {
    if (open && mode === "connect_ssh") {
      void loadSshConfigSuggestions();
    }
  }, [loadSshConfigSuggestions, mode, open]);

  const clearDiagnostics = useCallback(() => {
    setDiagnosticHostId(null);
    setDiagnosticLogs(EMPTY_DIAGNOSTIC_LOGS);
    setDiagnosticsLoading(false);
    setDiagnosticsVisible(false);
    setDiagnosticsError(null);
  }, []);

  const formatLogReadError = (label: string, error: unknown) => {
    const message = error instanceof Error ? error.message : String(error);
    return `[${label}] ${message}`;
  };

  const readRemoteDiagnostics = useCallback(async (hostId: string) => {
    const [appLog, errorLog, gatewayLog, gatewayErrorLog] = await Promise.all([
      api.remoteReadAppLog(hostId, DIAGNOSTIC_LOG_LINES).catch((error) => formatLogReadError("app.log", error)),
      api.remoteReadErrorLog(hostId, DIAGNOSTIC_LOG_LINES).catch((error) => formatLogReadError("error.log", error)),
      api.remoteReadGatewayLog(hostId, DIAGNOSTIC_LOG_LINES).catch((error) => formatLogReadError("gateway.log", error)),
      api.remoteReadGatewayErrorLog(hostId, DIAGNOSTIC_LOG_LINES).catch((error) => formatLogReadError("gateway.err.log", error)),
    ]);
    return { appLog, errorLog, gatewayLog, gatewayErrorLog };
  }, []);

  const readLocalDiagnostics = useCallback(async () => {
    const [appLog, errorLog, gatewayLog, gatewayErrorLog] = await Promise.all([
      api.readAppLog(DIAGNOSTIC_LOG_LINES).catch((error) => formatLogReadError("app.log", error)),
      api.readErrorLog(DIAGNOSTIC_LOG_LINES).catch((error) => formatLogReadError("error.log", error)),
      api.readGatewayLog(DIAGNOSTIC_LOG_LINES).catch((error) => formatLogReadError("gateway.log", error)),
      api.readGatewayErrorLog(DIAGNOSTIC_LOG_LINES).catch((error) => formatLogReadError("gateway.err.log", error)),
    ]);
    return { appLog, errorLog, gatewayLog, gatewayErrorLog };
  }, []);

  const refreshDiagnostics = useCallback(async (hostId: string | null) => {
    if (diagnosticsLoading) return;
    setDiagnosticsLoading(true);
    setDiagnosticsError(null);
    setDiagnosticsVisible(true);
    try {
      const logs = hostId
        ? await readRemoteDiagnostics(hostId)
        : await readLocalDiagnostics();
      setDiagnosticLogs(logs);
      setDiagnosticHostId(hostId);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setDiagnosticsError(message);
    } finally {
      setDiagnosticsLoading(false);
    }
  }, [diagnosticsLoading, readLocalDiagnostics, readRemoteDiagnostics]);

  const enterFailureMode = useCallback((error: unknown, hostId: string | null) => {
    const errorText = error instanceof Error ? error.message : String(error);
    const guidanceError = hasGuidanceEmitted(error) || isAlreadyExplainedGuidanceError(errorText);
    setDiagnosticHostId(hostId);
    setMode("failed");
    setRunError(guidanceError ? t("installChat.connectionFailed") : errorText);
    setRunErrorHasGuidance(guidanceError);
    void refreshDiagnostics(hostId);
  }, [refreshDiagnostics, t]);

  const readArtifactSession = (
    method: "remote_ssh" | "docker" | "wsl2",
    artifacts: Record<string, string>,
  ): InstallSession => {
    const now = new Date().toISOString();
    return {
      id: `install-${Date.now()}`,
      method,
      state: "ready",
      current_step: null,
      logs: [],
      artifacts,
      created_at: now,
      updated_at: now,
    };
  };

  const handleSshConnectSubmit = useCallback(async (host: SshHost) => {
    setConnectSubmitting(true);
    setRunError(null);
    clearDiagnostics();
    let targetHostId: string | null = null;
    try {
      const existingHosts = await withGuidance(
        () => api.listSshHosts(),
        "listSshHosts",
        "local",
        "local",
      ).catch(() => [] as SshHost[]);
      const requestedId = host.id?.trim();
      const idBase = requestedId || buildDefaultSshHostId(host);
      const existingIds = new Set(existingHosts.map((item) => item.id));
      let resolvedId = idBase;
      let suffix = 2;
      while (existingIds.has(resolvedId) && resolvedId !== requestedId) {
        resolvedId = `${idBase}-${suffix}`;
        suffix += 1;
      }
      if (!existingIds.has(resolvedId)) {
        clearRemotePersistenceScope(resolvedId);
      }
      targetHostId = resolvedId;
      const saved = await withGuidance(
        () => api.upsertSshHost({ ...host, id: resolvedId }),
        "upsertSshHost",
        resolvedId,
        "remote_ssh",
      );
      targetHostId = saved.id;
      if (connectRemoteHost) {
        await connectRemoteHost(saved.id);
      } else {
        await withGuidance(
          () => api.sshConnect(saved.id),
          "sshConnect",
          saved.id,
          "remote_ssh",
        );
      }
      ensureRemotePersistenceScope(saved);
      try {
        await withGuidance(
          () => api.remoteGetInstanceStatus(saved.id),
          "remoteGetInstanceStatus",
          saved.id,
          "remote_ssh",
        );
      } catch {
        // Remote openclaw may not be installed yet; connecting the host is enough here.
      }
      onReady?.(readArtifactSession("remote_ssh", {
        ssh_host_id: saved.id,
        ssh_host_label: saved.label,
      }));
    } catch (error) {
      enterFailureMode(error, targetHostId);
    } finally {
      setConnectSubmitting(false);
    }
  }, [clearDiagnostics, connectRemoteHost, enterFailureMode, onReady]);

  const handleDockerConnectSubmit = useCallback(async () => {
    setConnectSubmitting(true);
    setRunError(null);
    clearDiagnostics();
    try {
      const home = dockerConnectHome.trim();
      if (!home) throw new Error("Docker OpenClaw home is required");
      const label = dockerConnectLabel.trim() || undefined;
      const connected = await withGuidance(
        () => api.connectDockerInstance(home, label, undefined),
        "connectDockerInstance",
        "docker:manual",
        "docker_local",
      );
      onReady?.(readArtifactSession("docker", {
        docker_instance_id: connected.id,
        docker_instance_label: connected.label,
        docker_openclaw_home: connected.openclawHome || home,
        docker_clawpal_data_dir: connected.clawpalDataDir || "",
      }));
    } catch (error) {
      enterFailureMode(error, null);
    } finally {
      setConnectSubmitting(false);
    }
  }, [clearDiagnostics, dockerConnectHome, dockerConnectLabel, enterFailureMode, onReady]);

  const handleWsl2ConnectSubmit = useCallback(async () => {
    setConnectSubmitting(true);
    setRunError(null);
    clearDiagnostics();
    try {
      const home = wsl2ConnectHome.trim();
      if (!home) throw new Error("WSL2 OpenClaw home path is required");
      const baseId = `wsl2:${sanitizeLocalIdSegment(
        wsl2ConnectLabel.trim() || home.split(/[\\/]/).pop() || "default",
      )}`;
      const existing = await withGuidance(
        () => api.listRegisteredInstances(),
        "listRegisteredInstances",
        "local",
        "local",
      ).catch(() => [] as Array<{ id: string }>);
      const existingIds = new Set(existing.map((inst) => inst.id));
      let id = baseId;
      let suffix = 2;
      while (existingIds.has(id)) {
        id = `${baseId}-${suffix}`;
        suffix += 1;
      }
      const label = wsl2ConnectLabel.trim() || undefined;
      const connected = await withGuidance(
        () => api.connectLocalInstance(home, label, id),
        "connectLocalInstance",
        id || "wsl2:manual",
        "local",
      );
      onReady?.(readArtifactSession("wsl2", {
        local_instance_id: connected.id,
        local_instance_label: connected.label,
        local_openclaw_home: connected.openclawHome || home,
      }));
    } catch (error) {
      enterFailureMode(error, null);
    } finally {
      setConnectSubmitting(false);
    }
  }, [clearDiagnostics, enterFailureMode, onReady, wsl2ConnectHome, wsl2ConnectLabel]);

  const clearRunErrorState = () => {
    setRunError(null);
    setRunErrorHasGuidance(false);
    clearDiagnostics();
  };

  const diagnosticTargetLabel = diagnosticHostId
    ? t("installChat.diagnosticSourceRemote", { hostId: diagnosticHostId })
    : t("instance.local");

  const renderDiagnosticSection = (title: string, content: string) => (
    <details className="border border-border rounded-md" open>
      <summary className="px-3 py-2 cursor-pointer text-xs font-medium">
        {title}
      </summary>
      <pre className="px-3 pb-2 whitespace-pre-wrap break-words text-xs font-mono max-h-48 overflow-auto">
        {content || t("doctor.noLogs")}
      </pre>
    </details>
  );

  const runErrorPanel = runError ? (
    <div className="rounded-md border border-destructive/30 bg-destructive/5 text-destructive px-3 py-2 space-y-2">
      <p className="text-sm font-medium">{t("installChat.connectionFailed")}</p>
      {!runErrorHasGuidance ? <p className="text-sm whitespace-pre-wrap break-words">{runError}</p> : null}
      {runErrorHasGuidance ? (
        <p className="text-sm text-muted-foreground">{t("home.fixInDoctor")}</p>
      ) : null}
      <div className="flex flex-wrap gap-2">
        {onOpenDoctor ? (
          <Button type="button" size="sm" variant="outline" onClick={() => onOpenDoctor()}>
            {t("home.fixInDoctor")}
          </Button>
        ) : null}
        <Button
          type="button"
          size="sm"
          variant={diagnosticsVisible ? "secondary" : "outline"}
          onClick={() => setDiagnosticsVisible((value) => !value)}
        >
          {diagnosticsVisible ? t("doctor.collapse") : t("doctor.details")}
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => {
            void refreshDiagnostics(diagnosticHostId);
          }}
          disabled={diagnosticsLoading}
        >
          {t("doctor.refreshLogs")}
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">
        {t("doctor.appLog")} / {t("doctor.errorLog")} / {t("doctor.gatewayLogs")} {t("doctor.source")} {diagnosticTargetLabel}
      </p>
      {diagnosticsLoading ? (
        <p className="text-xs text-muted-foreground animate-pulse">{t("installChat.loadingDiagnostics")}</p>
      ) : null}
      {diagnosticsError ? (
        <p className="text-xs text-destructive">
          {t("installChat.diagnosticsError", { error: diagnosticsError })}
        </p>
      ) : null}
      {diagnosticsVisible ? (
        <div className="space-y-2">
          {renderDiagnosticSection(t("doctor.appLog"), diagnosticLogs.appLog)}
          {renderDiagnosticSection(t("doctor.errorLog"), diagnosticLogs.errorLog)}
          {renderDiagnosticSection(t("installChat.gatewayLogsApp"), diagnosticLogs.gatewayLog)}
          {renderDiagnosticSection(t("installChat.gatewayLogsError"), diagnosticLogs.gatewayErrorLog)}
        </div>
      ) : null}
    </div>
  ) : null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[80vh] flex flex-col overflow-hidden">
        <DialogHeader>
          <DialogTitle>
            {(mode === "connect_ssh" || mode === "connect_docker" || mode === "connect_wsl2")
              ? t("installChat.connectTitle")
              : t("installChat.title")}
          </DialogTitle>
          <DialogDescription className="sr-only">
            {(mode === "connect_ssh" || mode === "connect_docker" || mode === "connect_wsl2")
              ? t("installChat.connectTitle")
              : t("installChat.title")}
          </DialogDescription>
        </DialogHeader>

        {mode === "connect_ssh" ? (
          <div className="min-h-0 flex-1 overflow-y-auto space-y-4 py-2 pr-1">
            <div className="text-sm text-muted-foreground">
              {t("installChat.connectRemoteDescription")}
            </div>
            <SshFormWidget
              invokeId="connect-ssh-form"
              sshConfigSuggestions={sshConfigSuggestions}
              onSubmit={(_invokeId, host) => handleSshConnectSubmit(host)}
              onCancel={() => {
                setMode("idle");
                clearRunErrorState();
              }}
            />
            {sshConfigSuggestionsLoading ? (
              <div className="text-xs text-muted-foreground">
                {t("installChat.sshConfigPresetLoading")}
              </div>
            ) : null}
            {sshConfigSuggestionsError ? (
              <div className="text-xs text-destructive">{sshConfigSuggestionsError}</div>
            ) : null}
          </div>
        ) : mode === "connect_docker" ? (
          <div className="min-h-0 flex-1 overflow-y-auto space-y-4 py-2 pr-1">
            <div className="text-sm text-muted-foreground">{t("installChat.connectDockerDescription")}</div>
            <div className="space-y-1.5">
              <Label>{t("installChat.dockerHomeLabel")}</Label>
              <Input
                value={dockerConnectHome}
                onChange={(e) => setDockerConnectHome(e.target.value)}
                placeholder={t("installChat.dockerHomePlaceholder")}
              />
            </div>
            <div className="space-y-1.5">
              <Label>{t("installChat.dockerLabelLabel")}</Label>
              <Input
                value={dockerConnectLabel}
                onChange={(e) => setDockerConnectLabel(e.target.value)}
                placeholder={t("installChat.dockerLabelPlaceholder")}
              />
            </div>
            <DialogFooter>
              <Button
                variant="outline"
                onClick={() => {
                  setMode("idle");
                  clearRunErrorState();
                }}
                disabled={connectSubmitting}
              >
                {t("installChat.cancel")}
              </Button>
              <Button onClick={handleDockerConnectSubmit} disabled={connectSubmitting}>
                {t("installChat.submit")}
              </Button>
            </DialogFooter>
          </div>
        ) : mode === "connect_wsl2" ? (
          <div className="min-h-0 flex-1 overflow-y-auto space-y-4 py-2 pr-1">
            <div className="text-sm text-muted-foreground">{t("installChat.connectWsl2Description")}</div>
            <div className="space-y-1.5">
              <Label>{t("installChat.wsl2HomeLabel")}</Label>
              <Input
                value={wsl2ConnectHome}
                onChange={(e) => setWsl2ConnectHome(e.target.value)}
                placeholder={t("installChat.wsl2HomePlaceholder")}
              />
            </div>
            <div className="space-y-1.5">
              <Label>{t("installChat.wsl2LabelLabel")}</Label>
              <Input
                value={wsl2ConnectLabel}
                onChange={(e) => setWsl2ConnectLabel(e.target.value)}
                placeholder={t("installChat.wsl2LabelPlaceholder")}
              />
            </div>
            <DialogFooter>
              <Button
                variant="outline"
                onClick={() => {
                  setMode("idle");
                  clearRunErrorState();
                }}
                disabled={connectSubmitting}
              >
                {t("installChat.cancel")}
              </Button>
              <Button onClick={handleWsl2ConnectSubmit} disabled={connectSubmitting}>
                {t("installChat.submit")}
              </Button>
            </DialogFooter>
          </div>
        ) : mode === "failed" ? (
          <div className="space-y-4 py-2">
            {runErrorPanel}
            <DialogFooter>
              <Button
                variant="outline"
                onClick={() => {
                  setMode("idle");
                  clearRunErrorState();
                }}
              >
                {t("installChat.back")}
              </Button>
            </DialogFooter>
          </div>
        ) : (
          <div className="space-y-4 py-2">
            <div className="text-sm text-muted-foreground">
              {t("start.addInstanceHint")}
            </div>
            <InstallHubEntryCards
              onConnectRemote={() => setMode("connect_ssh")}
              onConnectDocker={() => setMode("connect_docker")}
              onConnectWsl2={() => setMode("connect_wsl2")}
            />
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
