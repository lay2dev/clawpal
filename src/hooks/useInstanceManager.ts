import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "@/lib/api";
import { withGuidance } from "@/lib/guidance";
import {
  deriveDockerPaths,
  deriveDockerLabel,
  normalizeDockerInstance,
} from "@/lib/docker-instance-helpers";
import { logDevIgnoredError } from "@/lib/dev-logging";
import type {
  DiscoveredInstance,
  DockerInstance,
  RegisteredInstance,
  SshHost,
} from "@/lib/types";

export function useInstanceManager() {
  const [sshHosts, setSshHosts] = useState<SshHost[]>([]);
  const [registeredInstances, setRegisteredInstances] = useState<RegisteredInstance[]>([]);
  const [discoveredInstances, setDiscoveredInstances] = useState<DiscoveredInstance[]>([]);
  const [discoveringInstances, setDiscoveringInstances] = useState(false);
  const [connectionStatus, setConnectionStatus] = useState<Record<string, "connected" | "disconnected" | "error">>({});
  const [sshEditOpen, setSshEditOpen] = useState(false);
  const [editingSshHost, setEditingSshHost] = useState<SshHost | null>(null);

  const handleEditSsh = useCallback((host: SshHost) => {
    setEditingSshHost(host);
    setSshEditOpen(true);
  }, []);

  const refreshHosts = useCallback(() => {
    withGuidance(() => api.listSshHosts(), "listSshHosts", "local", "local")
      .then(setSshHosts)
      .catch((error) => {
        logDevIgnoredError("refreshHosts", error);
      });
  }, []);

  const refreshRegisteredInstances = useCallback(() => {
    withGuidance(() => api.listRegisteredInstances(), "listRegisteredInstances", "local", "local")
      .then(setRegisteredInstances)
      .catch((error) => {
        logDevIgnoredError("listRegisteredInstances", error);
        setRegisteredInstances([]);
      });
  }, []);

  const discoverInstances = useCallback(() => {
    setDiscoveringInstances(true);
    withGuidance(
      () => api.discoverLocalInstances(),
      "discoverLocalInstances",
      "local",
      "local",
    )
      .then(setDiscoveredInstances)
      .catch((error) => {
        logDevIgnoredError("discoverLocalInstances", error);
        setDiscoveredInstances([]);
      })
      .finally(() => setDiscoveringInstances(false));
  }, []);

  const dockerInstances = useMemo<DockerInstance[]>(() => {
    const seen = new Set<string>();
    const out: DockerInstance[] = [];
    for (const item of registeredInstances) {
      if (item.instanceType !== "docker") continue;
      if (!item.id || seen.has(item.id)) continue;
      seen.add(item.id);
      out.push(normalizeDockerInstance({
        id: item.id,
        label: item.label || deriveDockerLabel(item.id),
        openclawHome: item.openclawHome || undefined,
        clawpalDataDir: item.clawpalDataDir || undefined,
      }));
    }
    return out;
  }, [registeredInstances]);

  const upsertDockerInstance = useCallback(async (instance: DockerInstance): Promise<RegisteredInstance> => {
    const normalized = normalizeDockerInstance(instance);
    const registered = await withGuidance(
      () => api.connectDockerInstance(
        normalized.openclawHome || deriveDockerPaths(normalized.id).openclawHome,
        normalized.label,
        normalized.id,
      ),
      "connectDockerInstance",
      normalized.id,
      "docker_local",
    );
    const updated = await withGuidance(
      () => api.listRegisteredInstances(),
      "listRegisteredInstances",
      "local",
      "local",
    ).catch((error) => {
      logDevIgnoredError("listRegisteredInstances after connect", error);
      return null;
    });
    if (updated) setRegisteredInstances(updated);
    return registered;
  }, []);

  const renameDockerInstance = useCallback((id: string, label: string) => {
    const nextLabel = label.trim();
    if (!nextLabel) return;
    const instance = dockerInstances.find((item) => item.id === id);
    if (!instance) return;
    void withGuidance(
      () => api.connectDockerInstance(
        instance.openclawHome || deriveDockerPaths(instance.id).openclawHome,
        nextLabel,
        instance.id,
      ),
      "connectDockerInstance",
      instance.id,
      "docker_local",
    ).then(() => {
      refreshRegisteredInstances();
    });
  }, [dockerInstances, refreshRegisteredInstances]);

  const deleteDockerInstance = useCallback(async (instance: DockerInstance, deleteLocalData: boolean) => {
    const fallback = deriveDockerPaths(instance.id);
    const openclawHome = instance.openclawHome || fallback.openclawHome;
    if (deleteLocalData) {
      await withGuidance(
        () => api.deleteLocalInstanceHome(openclawHome),
        "deleteLocalInstanceHome",
        instance.id,
        "docker_local",
      );
    }
    await withGuidance(
      () => api.deleteRegisteredInstance(instance.id),
      "deleteRegisteredInstance",
      instance.id,
      "docker_local",
    );
    refreshRegisteredInstances();
  }, [refreshRegisteredInstances]);

  useEffect(() => {
    refreshHosts();
    refreshRegisteredInstances();
    discoverInstances();
    const timer = setInterval(refreshRegisteredInstances, 30_000);
    return () => clearInterval(timer);
  }, [refreshHosts, refreshRegisteredInstances, discoverInstances]);

  return {
    sshHosts,
    registeredInstances,
    setRegisteredInstances,
    discoveredInstances,
    discoveringInstances,
    connectionStatus,
    setConnectionStatus,
    sshEditOpen,
    setSshEditOpen,
    editingSshHost,
    handleEditSsh,
    refreshHosts,
    refreshRegisteredInstances,
    discoverInstances,
    dockerInstances,
    upsertDockerInstance,
    renameDockerInstance,
    deleteDockerInstance,
  };
}
