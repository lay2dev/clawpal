import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "@/lib/api";
import { prewarmRemoteInstanceReadCache } from "@/lib/use-api";
import { withGuidance, explainAndBuildGuidanceError } from "@/lib/guidance";
import {
  ensureRemotePersistenceScope,
  readRemotePersistenceScope,
} from "@/lib/instance-persistence";
import {
  shouldEnableLocalInstanceScope,
} from "@/lib/instance-availability";
import { deriveDockerPaths, hashInstanceToken } from "@/lib/docker-instance-helpers";
import { logDevIgnoredError } from "@/lib/dev-logging";
import type { DockerInstance, RegisteredInstance, SshHost, PrecheckIssue } from "@/lib/types";


interface UseInstancePersistenceParams {
  activeInstance: string;
  registeredInstances: RegisteredInstance[];
  dockerInstances: DockerInstance[];
  sshHosts: SshHost[];
  isDocker: boolean;
  isRemote: boolean;
  isConnected: boolean;
  resolveInstanceTransport: (id: string) => "local" | "docker_local" | "remote_ssh";
  showToast: (message: string, type?: "success" | "error") => void;
}

export function useInstancePersistence(params: UseInstancePersistenceParams) {
  const {
    activeInstance,
    registeredInstances,
    dockerInstances,
    sshHosts,
    isDocker,
    isRemote,
    isConnected,
    resolveInstanceTransport,
    showToast,
  } = params;

  const [configVersion, setConfigVersion] = useState(0);
  const [instanceToken, setInstanceToken] = useState(0);
  const prevActiveInstanceRef = useRef(activeInstance);
  const [persistenceScope, setPersistenceScope] = useState<string | null>("local");
  const [persistenceResolved, setPersistenceResolved] = useState(true);

  // Synchronously resolve persistence scope for remote instances during render
  // to avoid a one-render lag that causes useChannelCache to load the wrong
  // instance's data.  React allows setState during render when guarded by a
  // condition — it bails out, discards the current output, and immediately
  // re-renders with the new value before effects fire or the browser paints.
  if (isRemote) {
    const host = sshHosts.find((item) => item.id === activeInstance) || null;
    const nextScope = host ? readRemotePersistenceScope(host) : null;
    if (persistenceScope !== nextScope || !persistenceResolved) {
      setPersistenceScope(nextScope);
      setPersistenceResolved(true);
    }
  }

  const accessProbeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastAccessProbeAtRef = useRef<Record<string, number>>({});

  const bumpConfigVersion = useCallback(() => {
    setConfigVersion((v) => v + 1);
  }, []);



  const ensureAccessForInstance = useCallback((instanceId: string) => {
    const transport = resolveInstanceTransport(instanceId);
    withGuidance(
      () => api.ensureAccessProfile(instanceId, transport),
      "ensureAccessProfile",
      instanceId,
      transport,
    ).catch((error) => {
      logDevIgnoredError("ensureAccessProfile", error);
    });
    withGuidance(
      () => api.precheckAuth(instanceId),
      "precheckAuth",
      instanceId,
      transport,
    ).then((issues) => {
      const errors = issues.filter((i: PrecheckIssue) => i.severity === "error");
      if (errors.length === 1) {
        showToast(errors[0].message, "error");
      } else if (errors.length > 1) {
        showToast(`${errors[0].message} (+${errors.length - 1} more)`, "error");
      }
    }).catch((error) => {
      logDevIgnoredError("precheckAuth", error);
    });
  }, [resolveInstanceTransport, showToast]);

  const scheduleEnsureAccessForInstance = useCallback((instanceId: string, delayMs = 1200) => {
    const now = Date.now();
    const last = lastAccessProbeAtRef.current[instanceId] || 0;
    if (now - last < 30_000) return;
    if (accessProbeTimerRef.current !== null) {
      clearTimeout(accessProbeTimerRef.current);
      accessProbeTimerRef.current = null;
    }
    accessProbeTimerRef.current = setTimeout(() => {
      lastAccessProbeAtRef.current[instanceId] = Date.now();
      ensureAccessForInstance(instanceId);
      accessProbeTimerRef.current = null;
    }, delayMs);
  }, [ensureAccessForInstance]);

  // Cleanup access probe timer
  useEffect(() => {
    return () => {
      if (accessProbeTimerRef.current !== null) {
        clearTimeout(accessProbeTimerRef.current);
        accessProbeTimerRef.current = null;
      }
    };
  }, []);

  // Global error handlers
  useEffect(() => {
    const handleUnhandled = (operation: string, reason: unknown) => {
      if (reason && typeof reason === "object" && (reason as any)._guidanceEmitted) {
        return;
      }
      const transport = resolveInstanceTransport(activeInstance);
      void explainAndBuildGuidanceError({
        method: operation,
        instanceId: activeInstance,
        transport,
        rawError: reason,
        emitEvent: true,
      });
      void api.captureFrontendError(
        typeof reason === "string" ? reason : String(reason),
        undefined,
        "error",
      ).catch(() => {});
    };

    const onUnhandledRejection = (event: PromiseRejectionEvent) => {
      logDevIgnoredError("unhandledRejection", event.reason);
      handleUnhandled("unhandledRejection", event.reason);
    };
    const onGlobalError = (event: ErrorEvent) => {
      const detail = event.error ?? event.message ?? "unknown error";
      logDevIgnoredError("unhandledError", detail);
      handleUnhandled("unhandledError", detail);
    };

    window.addEventListener("unhandledrejection", onUnhandledRejection);
    window.addEventListener("error", onGlobalError);
    return () => {
      window.removeEventListener("unhandledrejection", onUnhandledRejection);
      window.removeEventListener("error", onGlobalError);
    };
  }, [activeInstance, resolveInstanceTransport]);

  // Resolve persistence scope for active instance
  useEffect(() => {
    let cancelled = false;
    const resolvePersistence = async () => {
      if (isRemote) {
        const host = sshHosts.find((item) => item.id === activeInstance) || null;
        setPersistenceScope(host ? readRemotePersistenceScope(host) : null);
        setPersistenceResolved(true);
        return;
      }

      let openclawHome: string | null = null;
      const activeRegistered = registeredInstances.find((item) => item.id === activeInstance);
      if (activeInstance === "local") {
        openclawHome = "~";
      } else if (isDocker) {
        const instance = dockerInstances.find((item) => item.id === activeInstance);
        const fallback = deriveDockerPaths(activeInstance);
        openclawHome = instance?.openclawHome || fallback.openclawHome;
      } else if (activeRegistered?.instanceType === "local" && activeRegistered.openclawHome) {
        openclawHome = activeRegistered.openclawHome;
      }

      if (!openclawHome) {
        setPersistenceScope(null);
        setPersistenceResolved(true);
        return;
      }

      setPersistenceResolved(false);
      setPersistenceScope(null);
      try {
        const [exists, cliAvailable] = await Promise.all([
          api.localOpenclawConfigExists(openclawHome),
          api.localOpenclawCliAvailable(),
        ]);
        if (cancelled) return;
        setPersistenceScope(
          shouldEnableLocalInstanceScope({
            configExists: exists,
            cliAvailable,
          }) ? activeInstance : null,
        );
      } catch (error) {
        logDevIgnoredError("localOpenclawConfigExists", error);
        if (cancelled) return;
        setPersistenceScope(null);
      } finally {
        if (!cancelled) {
          setPersistenceResolved(true);
        }
      }
    };

    void resolvePersistence();
    return () => {
      cancelled = true;
    };
  }, [activeInstance, dockerInstances, isDocker, isRemote, registeredInstances, sshHosts]);

  // Sync remote persistence scope when connected
  useEffect(() => {
    if (!isRemote || !isConnected) return;
    const host = sshHosts.find((item) => item.id === activeInstance);
    if (!host) return;
    const nextScope = ensureRemotePersistenceScope(host);
    if (persistenceScope !== nextScope) {
      setPersistenceScope(nextScope);
    }
    if (!persistenceResolved) {
      setPersistenceResolved(true);
    }
  }, [activeInstance, isConnected, isRemote, persistenceResolved, persistenceScope, sshHosts]);

  // Set instance overrides and update instanceToken
  useEffect(() => {
    let cancelled = false;
    let nextHome: string | null = null;
    let nextDataDir: string | null = null;
    // Only reset token to 0 when the active instance actually changes.
    // Other dependency changes (e.g. registeredInstances array ref) should
    // recompute the token without an intermediate 0 that causes UI flicker.
    const instanceChanged = prevActiveInstanceRef.current !== activeInstance;
    prevActiveInstanceRef.current = activeInstance;
    if (instanceChanged) {
      setInstanceToken(0);
    }
    const activeRegistered = registeredInstances.find((item) => item.id === activeInstance);
    if (activeInstance === "local" || isRemote) {
      nextHome = null;
      nextDataDir = null;
    } else if (isDocker) {
      const instance = dockerInstances.find((item) => item.id === activeInstance);
      const fallback = deriveDockerPaths(activeInstance);
      nextHome = instance?.openclawHome || fallback.openclawHome;
      nextDataDir = instance?.clawpalDataDir || fallback.clawpalDataDir;
    } else if (activeRegistered?.instanceType === "local" && activeRegistered.openclawHome) {
      nextHome = activeRegistered.openclawHome;
      nextDataDir = activeRegistered.clawpalDataDir || null;
    }
    const tokenSeed = `${activeInstance}|${nextHome || ""}|${nextDataDir || ""}`;

    const applyOverrides = async () => {
      if (nextHome === null && nextDataDir === null) {
        await Promise.all([
          api.setActiveOpenclawHome(null).catch((error) => logDevIgnoredError("setActiveOpenclawHome", error)),
          api.setActiveClawpalDataDir(null).catch((error) => logDevIgnoredError("setActiveClawpalDataDir", error)),
        ]);
      } else {
        await Promise.all([
          api.setActiveOpenclawHome(nextHome).catch((error) => logDevIgnoredError("setActiveOpenclawHome", error)),
          api.setActiveClawpalDataDir(nextDataDir).catch((error) => logDevIgnoredError("setActiveClawpalDataDir", error)),
        ]);
      }
      if (!cancelled) {
        setInstanceToken(hashInstanceToken(tokenSeed));
      }
    };
    void applyOverrides();
    return () => {
      cancelled = true;
    };
  }, [activeInstance, isDocker, isRemote, dockerInstances, registeredInstances]);

  // Prewarm remote cache
  useEffect(() => {
    if (!isRemote || !isConnected || !instanceToken) return;
    prewarmRemoteInstanceReadCache(activeInstance, instanceToken, persistenceScope);
  }, [activeInstance, instanceToken, isConnected, isRemote, persistenceScope]);

  return {
    configVersion,
    bumpConfigVersion,
    instanceToken,
    persistenceScope,
    setPersistenceScope,
    persistenceResolved,
    setPersistenceResolved,
    ensureAccessForInstance,
    scheduleEnsureAccessForInstance,
  };
}
