import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/lib/api";
import { invalidateGlobalReadCache } from "@/lib/use-api";
import { withGuidance, explainAndBuildGuidanceError } from "@/lib/guidance";
import { ensureRemotePersistenceScope } from "@/lib/instance-persistence";
import {
  SSH_PASSPHRASE_RETRY_HINT,
  buildSshPassphraseCancelMessage,
  buildSshPassphraseConnectErrorMessage,
} from "@/lib/sshConnectErrors";
import { buildFriendlySshError, extractErrorText } from "@/lib/sshDiagnostic";
import { logDevException, logDevIgnoredError } from "@/lib/dev-logging";
import type { SshHost, PrecheckIssue } from "@/lib/types";

interface ProfileSyncStatus {
  phase: "idle" | "syncing" | "success" | "error";
  message: string;
  instanceId: string | null;
}

interface UseSshConnectionParams {
  activeInstance: string;
  sshHosts: SshHost[];
  isRemote: boolean;
  isConnected: boolean;
  connectionStatus: Record<string, "connected" | "disconnected" | "error">;
  setConnectionStatus: React.Dispatch<React.SetStateAction<Record<string, "connected" | "disconnected" | "error">>>;
  setPersistenceScope: (scope: string | null) => void;
  setPersistenceResolved: (resolved: boolean) => void;
  resolveInstanceTransport: (id: string) => string;
  showToast: (message: string, type?: "success" | "error") => void;
  scheduleEnsureAccessForInstance: (id: string, delayMs?: number) => void;
}

export function useSshConnection(params: UseSshConnectionParams) {
  const { t } = useTranslation();
  const {
    activeInstance,
    sshHosts,
    isRemote,
    isConnected,
    setConnectionStatus,
    setPersistenceScope,
    setPersistenceResolved,
    showToast,
    scheduleEnsureAccessForInstance,
  } = params;

  const [profileSyncStatus, setProfileSyncStatus] = useState<ProfileSyncStatus>({
    phase: "idle",
    message: "",
    instanceId: null,
  });
  const [showSshTransferSpeedUi, setShowSshTransferSpeedUi] = useState(false);
  const [sshTransferStats, setSshTransferStats] = useState<import("@/lib/types").SshTransferStats | null>(null);
  const [doctorNavPulse, setDoctorNavPulse] = useState(false);

  const sshHealthFailStreakRef = useRef<Record<string, number>>({});
  const doctorSshAutohealMuteUntilRef = useRef<Record<string, number>>({});
  const passphraseResolveRef = useRef<((value: string | null) => void) | null>(null);
  const remoteAuthSyncAtRef = useRef<Record<string, number>>({});

  const [passphraseHostLabel, setPassphraseHostLabel] = useState<string>("");
  const [passphraseOpen, setPassphraseOpen] = useState(false);
  const [passphraseInput, setPassphraseInput] = useState("");

  const requestPassphrase = useCallback((hostLabel: string): Promise<string | null> => {
    setPassphraseHostLabel(hostLabel);
    setPassphraseInput("");
    setPassphraseOpen(true);
    return new Promise((resolve) => {
      passphraseResolveRef.current = resolve;
    });
  }, []);

  const closePassphraseDialog = useCallback((value: string | null) => {
    setPassphraseOpen(false);
    const resolve = passphraseResolveRef.current;
    passphraseResolveRef.current = null;
    if (resolve) resolve(value);
  }, []);

  const connectWithPassphraseFallback = useCallback(async (hostId: string) => {
    const host = sshHosts.find((h) => h.id === hostId);
    const hostLabel = host?.label || host?.host || hostId;
    try {
      await api.sshConnect(hostId);
      if (host) {
        const nextScope = ensureRemotePersistenceScope(host);
        if (hostId === activeInstance) {
          setPersistenceScope(nextScope);
          setPersistenceResolved(true);
        }
      }
      return;
    } catch (err) {
      const raw = extractErrorText(err);
      if ((!host || host.authMethod !== "password") && SSH_PASSPHRASE_RETRY_HINT.test(raw)) {
        if (host?.passphrase && host.passphrase.length > 0) {
          const fallbackMessage = buildSshPassphraseConnectErrorMessage(raw, hostLabel, t);
          if (fallbackMessage) {
            throw new Error(fallbackMessage);
          }
          throw await explainAndBuildGuidanceError({
            method: "sshConnect",
            instanceId: hostId,
            transport: "remote_ssh",
            rawError: err,
          });
        }
        const passphrase = await requestPassphrase(hostLabel);
        if (passphrase !== null) {
          try {
            await withGuidance(
              () => api.sshConnectWithPassphrase(hostId, passphrase),
              "sshConnectWithPassphrase",
              hostId,
              "remote_ssh",
            );
            if (host) {
              const nextScope = ensureRemotePersistenceScope(host);
              if (hostId === activeInstance) {
                setPersistenceScope(nextScope);
                setPersistenceResolved(true);
              }
            }
            return;
          } catch (passphraseErr) {
            const passphraseRaw = extractErrorText(passphraseErr);
            const fallbackMessage = buildSshPassphraseConnectErrorMessage(
              passphraseRaw, hostLabel, t, { passphraseWasSubmitted: true },
            );
            if (fallbackMessage) {
              throw new Error(fallbackMessage);
            }
            throw await explainAndBuildGuidanceError({
              method: "sshConnectWithPassphrase",
              instanceId: hostId,
              transport: "remote_ssh",
              rawError: passphraseErr,
            });
          }
        } else {
          throw new Error(buildSshPassphraseCancelMessage(hostLabel, t));
        }
      }
      const fallbackMessage = buildSshPassphraseConnectErrorMessage(raw, hostLabel, t);
      if (fallbackMessage) {
        throw new Error(fallbackMessage);
      }
      throw await explainAndBuildGuidanceError({
        method: "sshConnect",
        instanceId: hostId,
        transport: "remote_ssh",
        rawError: err,
      });
    }
  }, [activeInstance, requestPassphrase, sshHosts, t, setPersistenceScope, setPersistenceResolved]);

  const syncRemoteAuthAfterConnect = useCallback(async (hostId: string) => {
    const now = Date.now();
    const last = remoteAuthSyncAtRef.current[hostId] || 0;
    if (now - last < 30_000) return;
    remoteAuthSyncAtRef.current[hostId] = now;
    setProfileSyncStatus({
      phase: "syncing",
      message: t("doctor.profileSyncStarted"),
      instanceId: hostId,
    });
    try {
      const result = await api.remoteSyncProfilesToLocalAuth(hostId);
      invalidateGlobalReadCache(["listModelProfiles", "resolveApiKeys"]);
      const localProfiles = await api.listModelProfiles().catch((error) => {
        logDevIgnoredError("syncRemoteAuthAfterConnect listModelProfiles", error);
        return [];
      });
      if (result.resolvedKeys > 0 || result.syncedProfiles > 0) {
        if (localProfiles.length > 0) {
          const message = t("doctor.profileSyncSuccessMessage", {
            syncedProfiles: result.syncedProfiles,
            resolvedKeys: result.resolvedKeys,
          });
          showToast(message, "success");
          setProfileSyncStatus({ phase: "success", message, instanceId: hostId });
        } else {
          const message = t("doctor.profileSyncNoLocalProfiles");
          showToast(message, "error");
          setProfileSyncStatus({ phase: "error", message, instanceId: hostId });
        }
      } else if (result.totalRemoteProfiles > 0) {
        const message = t("doctor.profileSyncNoUsableKeys");
        showToast(message, "error");
        setProfileSyncStatus({ phase: "error", message, instanceId: hostId });
      } else {
        const message = t("doctor.profileSyncNoProfiles");
        showToast(message, "error");
        setProfileSyncStatus({ phase: "error", message, instanceId: hostId });
      }
    } catch (e) {
      const message = t("doctor.profileSyncFailed", { error: String(e) });
      showToast(message, "error");
      setProfileSyncStatus({ phase: "error", message, instanceId: hostId });
    }
  }, [showToast, t]);

  // SSH self-healing: detect dropped connections and reconnect
  useEffect(() => {
    if (!isRemote) return;
    let cancelled = false;
    let inFlight = false;
    const hostId = activeInstance;
    const reportAutoHealFailure = (rawError: unknown) => {
      void explainAndBuildGuidanceError({
        method: "sshConnect",
        instanceId: hostId,
        transport: "remote_ssh",
        rawError: rawError,
        emitEvent: true,
      }).catch((error) => {
        logDevIgnoredError("autoheal explainAndBuildGuidanceError", error);
      });
      showToast(buildFriendlySshError(rawError, t), "error");
    };
    const markFailure = (rawError: unknown) => {
      if (cancelled) return;
      const mutedUntil = doctorSshAutohealMuteUntilRef.current[hostId] || 0;
      if (Date.now() < mutedUntil) {
        logDevIgnoredError("ssh autoheal muted during doctor flow", rawError);
        return;
      }
      const streak = (sshHealthFailStreakRef.current[hostId] || 0) + 1;
      sshHealthFailStreakRef.current[hostId] = streak;
      if (streak >= 2) {
        setConnectionStatus((prev) => ({ ...prev, [hostId]: "error" }));
        if (streak === 2) {
          reportAutoHealFailure(rawError);
        }
      }
    };

    const checkAndHeal = async () => {
      if (cancelled || inFlight) return;
      inFlight = true;
      try {
        const status = await api.sshStatus(hostId);
        if (cancelled) return;
        if (status === "connected") {
          sshHealthFailStreakRef.current[hostId] = 0;
          setConnectionStatus((prev) => ({ ...prev, [hostId]: "connected" }));
          return;
        }
        try {
          await connectWithPassphraseFallback(hostId);
          if (!cancelled) {
            sshHealthFailStreakRef.current[hostId] = 0;
            setConnectionStatus((prev) => ({ ...prev, [hostId]: "connected" }));
          }
        } catch (connectError) {
          markFailure(connectError);
        }
      } catch (statusError) {
        markFailure(statusError);
      } finally {
        inFlight = false;
      }
    };

    checkAndHeal();
    const timer = setInterval(checkAndHeal, 15_000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [activeInstance, isRemote, showToast, t, connectWithPassphraseFallback, setConnectionStatus]);

  // Mute autoheal during doctor assistant flow
  useEffect(() => {
    if (!isRemote) return;
    let disposed = false;
    const currentHostId = activeInstance;
    const unlistenPromise = listen<{ phase?: string }>("doctor:assistant-progress", (event) => {
      if (disposed) return;
      const phase = event.payload?.phase || "";
      const cooldownMs = phase === "cleanup" ? 45_000 : 30_000;
      doctorSshAutohealMuteUntilRef.current[currentHostId] = Date.now() + cooldownMs;
    });
    return () => {
      disposed = true;
      void unlistenPromise.then((unlisten) => unlisten()).catch((error) => {
        logDevIgnoredError("doctor progress unlisten", error);
      });
    };
  }, [activeInstance, isRemote]);

  // Poll SSH transfer stats
  useEffect(() => {
    if (!showSshTransferSpeedUi || !isRemote || !isConnected) {
      setSshTransferStats(null);
      return;
    }
    let cancelled = false;
    const poll = () => {
      api.getSshTransferStats(activeInstance)
        .then((stats) => {
          if (!cancelled) setSshTransferStats(stats);
        })
        .catch((error) => {
          logDevIgnoredError("getSshTransferStats", error);
          if (!cancelled) setSshTransferStats(null);
        });
    };
    poll();
    const timer = window.setInterval(poll, 1000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [activeInstance, isConnected, isRemote, showSshTransferSpeedUi]);

  return {
    profileSyncStatus,
    showSshTransferSpeedUi,
    setShowSshTransferSpeedUi,
    sshTransferStats,
    doctorNavPulse,
    setDoctorNavPulse,
    passphraseHostLabel,
    passphraseOpen,
    passphraseInput,
    setPassphraseInput,
    closePassphraseDialog,
    connectWithPassphraseFallback,
    syncRemoteAuthAfterConnect,
  };
}
