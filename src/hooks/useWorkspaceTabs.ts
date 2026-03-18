import { startTransition, useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { withGuidance } from "@/lib/guidance";
import { clearRemotePersistenceScope } from "@/lib/instance-persistence";
import { closeWorkspaceTab } from "@/lib/tabWorkspace";
import { buildFriendlySshError } from "@/lib/sshDiagnostic";
import { deriveDockerLabel } from "@/lib/docker-instance-helpers";
import { logDevIgnoredError } from "@/lib/dev-logging";
import { OPEN_TABS_STORAGE_KEY } from "@/lib/routes";
import type { Route } from "@/lib/routes";
import type { PrecheckIssue, RegisteredInstance, SshHost, InstallSession, DockerInstance } from "@/lib/types";

interface UseWorkspaceTabsParams {
  registeredInstances: RegisteredInstance[];
  setRegisteredInstances: React.Dispatch<React.SetStateAction<RegisteredInstance[]>>;
  sshHosts: SshHost[];
  dockerInstances: DockerInstance[];
  resolveInstanceTransport: (id: string) => "local" | "docker_local" | "remote_ssh";
  connectWithPassphraseFallback: (hostId: string) => Promise<void>;
  syncRemoteAuthAfterConnect: (hostId: string) => Promise<void>;
  scheduleEnsureAccessForInstance: (id: string, delayMs?: number) => void;
  upsertDockerInstance: (instance: DockerInstance) => Promise<RegisteredInstance>;
  refreshHosts: () => void;
  refreshRegisteredInstances: () => void;
  showToast: (message: string, type?: "success" | "error") => void;
  setConnectionStatus: React.Dispatch<React.SetStateAction<Record<string, "connected" | "disconnected" | "error">>>;
  navigateRoute: (next: Route) => void;
}

export function useWorkspaceTabs(params: UseWorkspaceTabsParams) {
  const { t } = useTranslation();
  const {
    registeredInstances,
    setRegisteredInstances,
    sshHosts,
    dockerInstances,
    resolveInstanceTransport,
    connectWithPassphraseFallback,
    syncRemoteAuthAfterConnect,
    scheduleEnsureAccessForInstance,
    upsertDockerInstance,
    refreshHosts,
    refreshRegisteredInstances,
    showToast,
    setConnectionStatus,
    navigateRoute,
  } = params;

  const [openTabIds, setOpenTabIds] = useState<string[]>(() => {
    try {
      const stored = localStorage.getItem(OPEN_TABS_STORAGE_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        if (Array.isArray(parsed) && parsed.length > 0) return parsed;
      }
    } catch {}
    return ["local"];
  });
  const [activeInstance, setActiveInstance] = useState("local");
  const [inStart, setInStart] = useState(true);
  const [startSection, setStartSection] = useState<"overview" | "profiles" | "settings">("overview");

  // Persist open tabs
  useEffect(() => {
    localStorage.setItem(OPEN_TABS_STORAGE_KEY, JSON.stringify(openTabIds));
  }, [openTabIds]);

  const openTab = useCallback((id: string) => {
    startTransition(() => {
      setOpenTabIds((prev) => prev.includes(id) ? prev : [...prev, id]);
      setActiveInstance(id);
      setInStart(false);
      navigateRoute("home");
    });
  }, [navigateRoute]);

  const closeTab = useCallback((id: string) => {
    setOpenTabIds((prevOpenTabIds) => {
      const nextState = closeWorkspaceTab({
        openTabIds: prevOpenTabIds,
        activeInstance,
        inStart,
        startSection,
      }, id);
      setActiveInstance(nextState.activeInstance);
      setInStart(nextState.inStart);
      setStartSection(nextState.startSection);
      return nextState.openTabIds;
    });
  }, [activeInstance, inStart, startSection]);

  const handleInstanceSelect = useCallback((id: string) => {
    if (id === activeInstance && !inStart) {
      return;
    }
    startTransition(() => {
      setActiveInstance(id);
      setOpenTabIds((prev) => prev.includes(id) ? prev : [...prev, id]);
      setInStart(false);
      navigateRoute("home");
    });
    // Instance switch precheck
    withGuidance(
      () => api.precheckInstance(id),
      "precheckInstance",
      id,
      resolveInstanceTransport(id),
    ).then((issues) => {
      const blocking = issues.filter((i: PrecheckIssue) => i.severity === "error");
      if (blocking.length === 1) {
        showToast(blocking[0].message, "error");
      } else if (blocking.length > 1) {
        showToast(`${blocking[0].message}${t("doctor.remainingIssues", { count: blocking.length - 1 })}`, "error");
      }
    }).catch((error) => {
      logDevIgnoredError("precheckInstance", error);
    });
    const transport = resolveInstanceTransport(id);
    if (transport !== "remote_ssh") {
      withGuidance(
        () => api.precheckTransport(id),
        "precheckTransport",
        id,
        transport,
      ).then((issues) => {
        const blocking = issues.filter((i: PrecheckIssue) => i.severity === "error");
        if (blocking.length === 1) {
          showToast(blocking[0].message, "error");
        } else if (blocking.length > 1) {
          showToast(`${blocking[0].message}${t("doctor.remainingIssues", { count: blocking.length - 1 })}`, "error");
        } else {
          const warnings = issues.filter((i: PrecheckIssue) => i.severity === "warn");
          if (warnings.length > 0) {
            showToast(warnings[0].message, "error");
          }
        }
      }).catch((error) => {
        logDevIgnoredError("precheckTransport", error);
      });
    }
    if (transport !== "remote_ssh") return;
    withGuidance(
      () => api.sshStatus(id),
      "sshStatus",
      id,
      "remote_ssh",
    )
      .then((status) => {
        if (status === "connected") {
          setConnectionStatus((prev) => ({ ...prev, [id]: "connected" }));
          scheduleEnsureAccessForInstance(id, 1500);
          void syncRemoteAuthAfterConnect(id);
        } else {
          return connectWithPassphraseFallback(id)
            .then(() => {
              setConnectionStatus((prev) => ({ ...prev, [id]: "connected" }));
              scheduleEnsureAccessForInstance(id, 1500);
              void syncRemoteAuthAfterConnect(id);
            });
        }
      })
      .catch((error) => {
        logDevIgnoredError("sshStatus or reconnect", error);
        connectWithPassphraseFallback(id)
          .then(() => {
            setConnectionStatus((prev) => ({ ...prev, [id]: "connected" }));
            scheduleEnsureAccessForInstance(id, 1500);
            void syncRemoteAuthAfterConnect(id);
          })
          .catch((e2) => {
            setConnectionStatus((prev) => ({ ...prev, [id]: "error" }));
            const friendly = buildFriendlySshError(e2, t);
            showToast(friendly, "error");
          });
      });
  }, [activeInstance, inStart, resolveInstanceTransport, scheduleEnsureAccessForInstance, connectWithPassphraseFallback, syncRemoteAuthAfterConnect, showToast, t, navigateRoute, setConnectionStatus]);

  const openTabs = useMemo(() => {
    const registryById = new Map(registeredInstances.map((item) => [item.id, item]));
    return openTabIds.flatMap((id) => {
      if (id === "local") return { id, label: t("instance.local"), type: "local" as const };
      const registered = registryById.get(id);
      if (registered) {
        const fallbackLabel = registered.instanceType === "docker" ? deriveDockerLabel(id) : id;
        return {
          id,
          label: registered.label || fallbackLabel,
          type: registered.instanceType === "remote_ssh" ? "ssh" as const : registered.instanceType as "local" | "docker",
        };
      }
      return [];
    });
  }, [openTabIds, registeredInstances, t]);

  const openControlCenter = useCallback(() => {
    setInStart(true);
    setStartSection("overview");
  }, []);

  // Handle install completion
  const handleInstallReady = useCallback(async (session: InstallSession) => {
    const artifacts = session.artifacts || {};
    const readArtifactString = (keys: string[]): string => {
      for (const key of keys) {
        const value = artifacts[key];
        if (typeof value === "string" && value.trim()) {
          return value.trim();
        }
      }
      return "";
    };
    if (session.method === "docker") {
      const { deriveDockerPaths, DEFAULT_DOCKER_INSTANCE_ID } = await import("@/lib/docker-instance-helpers");
      const artifactId = readArtifactString(["docker_instance_id", "dockerInstanceId"]);
      const id = artifactId || DEFAULT_DOCKER_INSTANCE_ID;
      const fallback = deriveDockerPaths(id);
      const openclawHome = readArtifactString(["docker_openclaw_home", "dockerOpenclawHome"]) || fallback.openclawHome;
      const clawpalDataDir = readArtifactString(["docker_clawpal_data_dir", "dockerClawpalDataDir"]) || `${openclawHome}/data`;
      const label = readArtifactString(["docker_instance_label", "dockerInstanceLabel"]) || deriveDockerLabel(id);
      const registered = await upsertDockerInstance({ id, label, openclawHome, clawpalDataDir });
      openTab(registered.id);
    } else if (session.method === "remote_ssh") {
      let hostId = readArtifactString(["ssh_host_id", "sshHostId", "host_id", "hostId"]);
      const hostLabel = readArtifactString(["ssh_host_label", "sshHostLabel", "host_label", "hostLabel"]);
      const hostAddr = readArtifactString(["ssh_host", "sshHost", "host"]);
      if (!hostId) {
        const knownHosts = await api.listSshHosts().catch((error) => {
          logDevIgnoredError("handleInstallReady listSshHosts", error);
          return [] as SshHost[];
        });
        if (hostLabel) {
          const byLabel = knownHosts.find((item) => item.label === hostLabel);
          if (byLabel) hostId = byLabel.id;
        }
        if (!hostId && hostAddr) {
          const byHost = knownHosts.find((item) => item.host === hostAddr);
          if (byHost) hostId = byHost.id;
        }
      }
      if (hostId) {
        const activateRemoteInstance = (instanceId: string, status: "connected" | "error") => {
          setOpenTabIds((prev) => prev.includes(instanceId) ? prev : [...prev, instanceId]);
          setActiveInstance(instanceId);
          setConnectionStatus((prev) => ({ ...prev, [instanceId]: status }));
          setInStart(false);
          navigateRoute("home");
        };
        try {
          const instance = await withGuidance(
            () => api.connectSshInstance(hostId),
            "connectSshInstance",
            hostId,
            "remote_ssh",
          );
          setRegisteredInstances((prev) => {
            const filtered = prev.filter((r) => r.id !== hostId && r.id !== instance.id);
            return [...filtered, instance];
          });
          refreshHosts();
          refreshRegisteredInstances();
          activateRemoteInstance(instance.id, "connected");
          scheduleEnsureAccessForInstance(instance.id, 600);
          void syncRemoteAuthAfterConnect(instance.id);
        } catch (err) {
          console.warn("connectSshInstance failed during install-ready:", err);
          refreshHosts();
          refreshRegisteredInstances();
          const alreadyRegistered = registeredInstances.some((item) => item.id === hostId);
          if (alreadyRegistered) {
            activateRemoteInstance(hostId, "error");
          } else {
            setInStart(true);
            setStartSection("overview");
          }
          const reason = buildFriendlySshError(err, t);
          showToast(reason, "error");
        }
      } else {
        showToast("SSH host id missing after submit. Please reopen Connect and retry.", "error");
      }
    } else {
      openTab("local");
    }
  }, [
    upsertDockerInstance,
    openTab,
    refreshHosts,
    refreshRegisteredInstances,
    navigateRoute,
    registeredInstances,
    scheduleEnsureAccessForInstance,
    syncRemoteAuthAfterConnect,
    showToast,
    t,
    setConnectionStatus,
    setRegisteredInstances,
  ]);

  const handleDeleteSsh = useCallback((hostId: string) => {
    withGuidance(
      () => api.deleteSshHost(hostId),
      "deleteSshHost",
      hostId,
      "remote_ssh",
    ).then(() => {
      clearRemotePersistenceScope(hostId);
      closeTab(hostId);
      refreshHosts();
      refreshRegisteredInstances();
    }).catch((e) => console.warn("deleteSshHost:", e));
  }, [closeTab, refreshHosts, refreshRegisteredInstances]);

  return {
    openTabIds,
    setOpenTabIds,
    activeInstance,
    setActiveInstance,
    inStart,
    setInStart,
    startSection,
    setStartSection,
    openTab,
    closeTab,
    handleInstanceSelect,
    openTabs,
    openControlCenter,
    handleInstallReady,
    handleDeleteSsh,
  };
}
