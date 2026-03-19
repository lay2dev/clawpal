import { useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useInstance } from "./instance-context";
import { api } from "./api";
import {
  explainAndBuildGuidanceError,
} from "./guidance";
import { extractErrorText } from "./sshDiagnostic";
import {
  createDataLoadRequestId,
  emitDataLoadMetric,
  inferDataLoadPage,
  inferDataLoadSource,
  parseInstanceToken,
} from "./data-load-log";
import { writePersistedReadCache } from "./persistent-read-cache";
import {
  resolveReadCacheScopeKey, setOptimisticReadCache, shouldLogRemoteInvokeMetric, callWithReadCache, invalidateReadCacheForInstance, emitRemoteInvokeMetric, logDevApiError, makeCacheKey
} from "./api-read-cache";

// Re-export cache utilities consumed by other modules
export {
  hasGuidanceEmitted,
  subscribeToCacheKey,
  readCacheValue,
  buildCacheKey,
  resolveReadCacheScopeKey,
  invalidateGlobalReadCache,
  setOptimisticReadCache,
  primeReadCache,
  prewarmRemoteInstanceReadCache,
  shouldLogRemoteInvokeMetric,
} from "./api-read-cache";

export function useApi() {
  const {
    instanceId,
    instanceViewToken,
    instanceToken,
    isRemote,
    isDocker,
    isConnected,
    persistenceScope,
    persistenceResolved,
    channelNodes,
    discordGuildChannels,
    channelsLoading,
    discordChannelsLoading,
    refreshChannelNodesCache,
    refreshDiscordChannelsCache,
  } = useInstance();
  const instanceCacheKey = `${instanceId}#${instanceToken}`;
  const globalCacheKey = "__global__";
  const transport: "local" | "docker_local" | "remote_ssh" = isRemote
    ? "remote_ssh"
    : (isDocker ? "docker_local" : "local");
  const persistedReadScope = persistenceScope;

  const explainAndWrapError = useCallback(
    async (method: string | undefined, rawError: unknown) => {
      return explainAndBuildGuidanceError({
        method: method || "unknown",
        instanceId,
        transport,
        rawError,
        emitEvent: true,
      });
    },
    [instanceId, transport],
  );

  const dispatch = useCallback(
    <TArgs extends unknown[], TResult>(
      localFn: (...args: TArgs) => Promise<TResult>,
      remoteFn: (hostId: string, ...args: TArgs) => Promise<TResult>,
      method?: string,
    ) => {
      return (...args: TArgs): Promise<TResult> => {
        if (isRemote) {
          if (!isConnected) {
            return Promise.reject(
              new Error("Not connected to remote instance"),
            );
          }
          const startedAt = Date.now();
          return remoteFn(instanceId, ...args)
            .then((result) => {
              const elapsedMs = Date.now() - startedAt;
              if (shouldLogRemoteInvokeMetric(true, elapsedMs)) {
              emitRemoteInvokeMetric({
                method: method || "unknown",
                instanceId,
                argsCount: args.length,
                ok: true,
                elapsedMs,
              });
              }
              return result;
            })
            .catch(async (error) => {
              logDevApiError("useApi dispatch remote catch", error, {
                method: method || "unknown",
                transport,
                instanceId,
                argsCount: args.length,
              });
              const elapsedMs = Date.now() - startedAt;
              if (shouldLogRemoteInvokeMetric(false, elapsedMs)) {
              emitRemoteInvokeMetric({
                method: method || "unknown",
                instanceId,
                argsCount: args.length,
                ok: false,
                elapsedMs,
                error: extractErrorText(error),
              });
              }
              throw await explainAndWrapError(method, error);
            });
        }
        if (isDocker) {
          return localFn(...args).catch(async (error) => {
            logDevApiError("useApi dispatch local catch (docker)", error, {
              method: method || "unknown",
              transport,
              argsCount: args.length,
            });
            throw await explainAndWrapError(method, error);
          });
        }
        return localFn(...args).catch(async (error) => {
          logDevApiError("useApi dispatch local catch", error, {
            method: method || "unknown",
            transport,
            argsCount: args.length,
          });
          throw await explainAndWrapError(method, error);
        });
      };
    },
    [instanceId, isRemote, isDocker, isConnected, explainAndWrapError],
  );

  const dispatchCached = useCallback(
    <TArgs extends unknown[], TResult>(
      method: string,
      ttlMs: number,
      localFn: (...args: TArgs) => Promise<TResult>,
      remoteFn: (hostId: string, ...args: TArgs) => Promise<TResult>,
    ) => {
      const call = dispatch(localFn, remoteFn, method);
      return (...args: TArgs): Promise<TResult> =>
        callWithReadCache(
          resolveReadCacheScopeKey(instanceCacheKey, persistedReadScope, method),
          instanceId,
          persistedReadScope,
          method,
          args,
          ttlMs,
          () => call(...args),
        );
    },
    [dispatch, instanceCacheKey, instanceId, persistedReadScope],
  );

  const localCached = useCallback(
    <TArgs extends unknown[], TResult>(
      method: string,
      ttlMs: number,
      fn: (...args: TArgs) => Promise<TResult>,
    ) => {
      return (...args: TArgs): Promise<TResult> =>
        callWithReadCache(instanceCacheKey, instanceId, persistedReadScope, method, args, ttlMs, () => fn(...args));
    },
    [instanceCacheKey, instanceId, persistedReadScope],
  );

  const localGlobalCached = useCallback(
    <TArgs extends unknown[], TResult>(
      method: string,
      ttlMs: number,
      fn: (...args: TArgs) => Promise<TResult>,
    ) => {
      return (...args: TArgs): Promise<TResult> =>
        callWithReadCache(globalCacheKey, globalCacheKey, globalCacheKey, method, args, ttlMs, () => fn(...args));
    },
    [globalCacheKey],
  );

  const withInvalidation = useCallback(
    <TArgs extends unknown[], TResult>(
      fn: (...args: TArgs) => Promise<TResult>,
      methodsToInvalidate?: string[],
    ) => {
      return (...args: TArgs): Promise<TResult> =>
        fn(...args).then((result) => {
          invalidateReadCacheForInstance(instanceCacheKey, methodsToInvalidate);
          if (persistedReadScope && persistedReadScope !== instanceCacheKey) {
            invalidateReadCacheForInstance(persistedReadScope, methodsToInvalidate);
          }
          return result;
        });
    },
    [instanceCacheKey, persistedReadScope],
  );

  const withGlobalInvalidation = useCallback(
    <TArgs extends unknown[], TResult>(
      fn: (...args: TArgs) => Promise<TResult>,
      methodsToInvalidate?: string[],
    ) => {
      return (...args: TArgs): Promise<TResult> =>
        fn(...args).then((result) => {
          invalidateReadCacheForInstance(instanceCacheKey, methodsToInvalidate);
          invalidateReadCacheForInstance(globalCacheKey, methodsToInvalidate);
          return result;
        });
    },
    [instanceCacheKey, globalCacheKey],
  );

  /**
   * Pin an optimistic value in the read cache for a specific API method.
   * While pinned (default 15s), polling calls to the same method will
   * return the pinned value instead of overwriting with stale backend data.
   *
   * Usage: ua.pinOptimistic("listAgents", agents.filter(a => a.id !== deletedId));
   */
  const pinOptimistic = useCallback(
    <T,>(method: string, value: T, args: unknown[] = [], pinDurationMs = 15_000) => {
      const key = makeCacheKey(instanceCacheKey, method, args);
      setOptimisticReadCache(key, value, pinDurationMs);
    },
    [instanceCacheKey],
  );

  /** Pin an optimistic value in the global cache (for methods like listModelProfiles). */
  const pinOptimisticGlobal = useCallback(
    <T,>(method: string, value: T, args: unknown[] = [], pinDurationMs = 15_000) => {
      const key = makeCacheKey(globalCacheKey, method, args);
      setOptimisticReadCache(key, value, pinDurationMs);
    },
    [globalCacheKey],
  );

  return useMemo(
    () => ({
      // Instance state
      instanceId,
      instanceViewToken,
      instanceToken,
      instanceCacheKey,
      persistenceScope,
      persistenceResolved,
      isRemote,
      isDocker,
      isConnected,
      channelNodes,
      discordGuildChannels,
      channelsLoading,
      discordChannelsLoading,
      refreshChannelNodesCache,
      refreshDiscordChannelsCache,

      // Optimistic cache pinning
      pinOptimistic,
      pinOptimisticGlobal,

      // Status
      getInstanceStatus: dispatch(
        api.getInstanceStatus,
        api.remoteGetInstanceStatus,
      ),
      getInstanceConfigSnapshot: dispatchCached(
        "getInstanceConfigSnapshot",
        isRemote ? 20_000 : 12_000,
        api.getInstanceConfigSnapshot,
        api.remoteGetInstanceConfigSnapshot,
      ),
      getInstanceRuntimeSnapshot: dispatchCached(
        "getInstanceRuntimeSnapshot",
        isRemote ? 10_000 : 6_000,
        api.getInstanceRuntimeSnapshot,
        api.remoteGetInstanceRuntimeSnapshot,
      ),
      getStatusExtra: dispatchCached(
        "getStatusExtra",
        isRemote ? 15_000 : 10_000,
        api.getStatusExtra,
        api.remoteGetStatusExtra,
      ),

      // Agents
      listAgents: dispatchCached(
        "listAgents",
        isRemote ? 12_000 : 6_000,
        api.listAgentsOverview,
        api.remoteListAgentsOverview,
      ),
      setupAgentIdentity: dispatch(
        api.setupAgentIdentity,
        api.remoteSetupAgentIdentity,
      ),

      // Channels
      listChannels: dispatchCached(
        "listChannels",
        isRemote ? 15_000 : 8_000,
        api.listChannelsMinimal,
        api.remoteListChannelsMinimal,
      ),
      getChannelsConfigSnapshot: dispatchCached(
        "getChannelsConfigSnapshot",
        isRemote ? 20_000 : 12_000,
        api.getChannelsConfigSnapshot,
        api.remoteGetChannelsConfigSnapshot,
      ),
      getChannelsRuntimeSnapshot: dispatchCached(
        "getChannelsRuntimeSnapshot",
        isRemote ? 12_000 : 8_000,
        api.getChannelsRuntimeSnapshot,
        api.remoteGetChannelsRuntimeSnapshot,
      ),
      listBindings: dispatchCached(
        "listBindings",
        isRemote ? 12_000 : 8_000,
        api.listBindings,
        api.remoteListBindings,
      ),
      listDiscordGuildChannels: dispatchCached(
        "listDiscordGuildChannels",
        isRemote ? 20_000 : 12_000,
        api.listDiscordGuildChannels,
        api.remoteListDiscordGuildChannels,
      ),
      // Remote has no separate refresh command; reuse list which fetches fresh data
      refreshDiscordGuildChannels: dispatch(
        api.refreshDiscordGuildChannels,
        api.remoteListDiscordGuildChannels,
      ),

      // Models
      listModelProfiles: localGlobalCached(
        "listModelProfiles",
        10_000,
        api.listModelProfiles,
      ),
      upsertModelProfile: withGlobalInvalidation(
        api.upsertModelProfile,
      ),
      deleteModelProfile: withGlobalInvalidation(
        api.deleteModelProfile,
      ),
      // Profile credential validation uses local model profiles and local credentials only.
      // Avoid SSH hop here to keep test latency low.
      testModelProfile: (profileId: string) => api.testModelProfile(profileId),
      resolveApiKeys: localGlobalCached(
        "resolveApiKeys",
        10_000,
        api.resolveApiKeys,
      ),
      extractModelProfilesFromConfig: withGlobalInvalidation(
        api.extractModelProfilesFromConfig,
        ["listModelProfiles", "resolveApiKeys"],
      ),
      refreshModelCatalog: dispatch(
        api.refreshModelCatalog,
        api.remoteRefreshModelCatalog,
      ),

      // Config
      readRawConfig: dispatch(api.readRawConfig, api.remoteReadRawConfig),
      applyConfigPatch: withInvalidation(
        dispatch(
          api.applyConfigPatch,
          api.remoteApplyConfigPatch,
        ),
      ),
      restartGateway: withInvalidation(
        dispatch(api.restartGateway, api.remoteRestartGateway),
        ["getInstanceStatus", "getStatusExtra", "getInstanceRuntimeSnapshot", "getRescueBotStatus"],
      ),
      diagnoseDoctorAssistant: dispatch(
        api.diagnoseDoctorAssistant,
        api.remoteDiagnoseDoctorAssistant,
        "diagnoseDoctorAssistant",
      ),
      repairDoctorAssistant: dispatch(
        api.repairDoctorAssistant,
        api.remoteRepairDoctorAssistant,
        "repairDoctorAssistant",
      ),
      startRemoteDoctorRepair: () => api.startRemoteDoctorRepair(
        instanceId,
        isRemote ? "remote_openclaw" : "local_openclaw",
      ),
      getRescueBotStatus: dispatchCached(
        "getRescueBotStatus",
        isRemote ? 8_000 : 5_000,
        api.getRescueBotStatus,
        api.remoteGetRescueBotStatus,
      ),
      manageRescueBot: withInvalidation(
        dispatch(api.manageRescueBot, api.remoteManageRescueBot),
        ["getInstanceStatus", "getStatusExtra", "getInstanceRuntimeSnapshot", "getRescueBotStatus"],
      ),
      diagnosePrimaryViaRescue: dispatch(
        api.diagnosePrimaryViaRescue,
        api.remoteDiagnosePrimaryViaRescue,
      ),
      repairPrimaryViaRescue: dispatch(
        api.repairPrimaryViaRescue,
        api.remoteRepairPrimaryViaRescue,
      ),

      // Doctor
      runDoctor: dispatch(api.runDoctor, api.remoteRunDoctor),
      fixIssues: withInvalidation(dispatch(api.fixIssues, api.remoteFixIssues)),

      // History
      listHistory: dispatchCached(
        "listHistory",
        isRemote ? 12_000 : 8_000,
        api.listHistory,
        api.remoteListHistory,
      ),
      previewRollback: dispatch(
        api.previewRollback,
        api.remotePreviewRollback,
      ),
      rollback: withInvalidation(dispatch(api.rollback, api.remoteRollback)),

      // Sessions
      analyzeSessions: dispatch(
        api.analyzeSessions,
        api.remoteAnalyzeSessions,
      ),
      deleteSessionsByIds: withInvalidation(
        dispatch(
          api.deleteSessionsByIds,
          api.remoteDeleteSessionsByIds,
        ),
        ["listSessionFiles"],
      ),
      listSessionFiles: dispatchCached(
        "listSessionFiles",
        isRemote ? 15_000 : 10_000,
        api.listSessionFiles,
        api.remoteListSessionFiles,
      ),
      clearAllSessions: withInvalidation(
        dispatch(
          api.clearAllSessions,
          api.remoteClearAllSessions,
        ),
        ["listSessionFiles"],
      ),
      previewSession: dispatch(api.previewSession, api.remotePreviewSession),

      // Chat
      chatViaOpenclaw: dispatch(
        api.chatViaOpenclaw,
        api.remoteChatViaOpenclaw,
        "chatViaOpenclaw",
      ),

      // Backup & Upgrade
      backupBeforeUpgrade: dispatch(
        api.backupBeforeUpgrade,
        api.remoteBackupBeforeUpgrade,
      ),
      listBackups: dispatchCached(
        "listBackups",
        isRemote ? 20_000 : 12_000,
        api.listBackups,
        api.remoteListBackups,
      ),
      restoreFromBackup: withInvalidation(
        dispatch(
          api.restoreFromBackup,
          api.remoteRestoreFromBackup,
        ),
      ),
      deleteBackup: withInvalidation(
        dispatch(api.deleteBackup, api.remoteDeleteBackup),
        ["listBackups"],
      ),
      runOpenclawUpgrade: withInvalidation(
        dispatch(
          api.runOpenclawUpgrade,
          api.remoteRunOpenclawUpgrade,
        ),
        ["getStatusExtra", "checkOpenclawUpdate", "getCachedModelCatalog"],
      ),
      checkOpenclawUpdate: dispatchCached(
        "checkOpenclawUpdate",
        isRemote ? 10 * 60_000 : 30 * 60_000,
        api.checkOpenclawUpdate,
        api.remoteCheckOpenclawUpdate,
      ),

      // Cron & Watchdog
      listCronJobs: dispatchCached(
        "listCronJobs",
        isRemote ? 12_000 : 8_000,
        api.listCronJobs,
        api.remoteListCronJobs,
      ),
      getCronConfigSnapshot: dispatchCached(
        "getCronConfigSnapshot",
        isRemote ? 20_000 : 12_000,
        api.getCronConfigSnapshot,
        api.remoteGetCronConfigSnapshot,
      ),
      getCronRuntimeSnapshot: dispatchCached(
        "getCronRuntimeSnapshot",
        isRemote ? 12_000 : 8_000,
        api.getCronRuntimeSnapshot,
        api.remoteGetCronRuntimeSnapshot,
      ),
      getCronRuns: dispatchCached(
        "getCronRuns",
        isRemote ? 8_000 : 5_000,
        api.getCronRuns,
        api.remoteGetCronRuns,
      ),
      triggerCronJob: withInvalidation(
        dispatch(api.triggerCronJob, api.remoteTriggerCronJob),
        ["listCronJobs", "getCronConfigSnapshot", "getCronRuntimeSnapshot", "getCronRuns", "getWatchdogStatus"],
      ),
      deleteCronJob: withInvalidation(
        dispatch(api.deleteCronJob, api.remoteDeleteCronJob),
        ["listCronJobs", "getCronConfigSnapshot", "getCronRuntimeSnapshot", "getCronRuns", "getWatchdogStatus"],
      ),
      getWatchdogStatus: dispatchCached(
        "getWatchdogStatus",
        isRemote ? 8_000 : 5_000,
        api.getWatchdogStatus,
        api.remoteGetWatchdogStatus,
      ),
      deployWatchdog: withInvalidation(
        dispatch(api.deployWatchdog, api.remoteDeployWatchdog),
        ["getWatchdogStatus", "listCronJobs", "getCronRuntimeSnapshot"],
      ),
      startWatchdog: withInvalidation(
        dispatch(api.startWatchdog, api.remoteStartWatchdog),
        ["getWatchdogStatus", "listCronJobs", "getCronRuntimeSnapshot"],
      ),
      stopWatchdog: withInvalidation(
        dispatch(api.stopWatchdog, api.remoteStopWatchdog),
        ["getWatchdogStatus", "listCronJobs", "getCronRuntimeSnapshot"],
      ),
      uninstallWatchdog: withInvalidation(
        dispatch(
          api.uninstallWatchdog,
          api.remoteUninstallWatchdog,
        ),
        ["getWatchdogStatus", "listCronJobs", "getCronRuntimeSnapshot"],
      ),

      // Queue
      queueCommand: withInvalidation(
        dispatch(api.queueCommand, api.remoteQueueCommand),
        ["listQueuedCommands", "queuedCommandsCount", "previewQueuedCommands"],
      ),
      removeQueuedCommand: withInvalidation(
        dispatch(api.removeQueuedCommand, api.remoteRemoveQueuedCommand),
        ["listQueuedCommands", "queuedCommandsCount", "previewQueuedCommands"],
      ),
      listQueuedCommands: dispatch(api.listQueuedCommands, api.remoteListQueuedCommands),
      discardQueuedCommands: withInvalidation(
        dispatch(api.discardQueuedCommands, api.remoteDiscardQueuedCommands),
        ["listQueuedCommands", "queuedCommandsCount", "previewQueuedCommands"],
      ),
      previewQueuedCommands: dispatch(api.previewQueuedCommands, api.remotePreviewQueuedCommands),
      applyQueuedCommands: withInvalidation(
        dispatch(api.applyQueuedCommands, api.remoteApplyQueuedCommands),
        ["listQueuedCommands", "queuedCommandsCount", "previewQueuedCommands"],
      ),
      queuedCommandsCount: dispatch(api.queuedCommandsCount, api.remoteQueuedCommandsCount),

      // Logs
      readAppLog: dispatch(api.readAppLog, api.remoteReadAppLog),
      readErrorLog: dispatch(api.readErrorLog, api.remoteReadErrorLog),
      readGatewayLog: dispatch(api.readGatewayLog, api.remoteReadGatewayLog),
      readGatewayErrorLog: dispatch(api.readGatewayErrorLog, api.remoteReadGatewayErrorLog),
      readHelperLog: dispatch(api.readHelperLog, api.remoteReadHelperLog),

      // Local-only (no remote equivalent needed)
      getAppPreferences: localGlobalCached(
        "getAppPreferences",
        10_000,
        api.getAppPreferences,
      ),
      getBugReportSettings: localGlobalCached(
        "getBugReportSettings",
        5_000,
        api.getBugReportSettings,
      ),
      setBugReportSettings: withGlobalInvalidation(
        api.setBugReportSettings,
        ["getBugReportSettings", "getBugReportStats"],
      ),
      getBugReportStats: localGlobalCached(
        "getBugReportStats",
        2_000,
        api.getBugReportStats,
      ),
      testBugReportConnection: withGlobalInvalidation(
        api.testBugReportConnection,
        ["getBugReportStats"],
      ),
      setSshTransferSpeedUiPreference: withGlobalInvalidation(
        api.setSshTransferSpeedUiPreference,
        ["getAppPreferences"],
      ),
      setRemoteDoctorGatewayUrlPreference: withGlobalInvalidation(
        api.setRemoteDoctorGatewayUrlPreference,
        ["getAppPreferences"],
      ),
      setRemoteDoctorGatewayAuthTokenPreference: withGlobalInvalidation(
        api.setRemoteDoctorGatewayAuthTokenPreference,
        ["getAppPreferences"],
      ),
      ensureAccessProfile: api.ensureAccessProfile,
      recordInstallExperience: api.recordInstallExperience,
      openUrl: api.openUrl,
      resolveProviderAuth: api.resolveProviderAuth,
      getCachedModelCatalog: localCached(
        "getCachedModelCatalog",
        8_000,
        api.getCachedModelCatalog,
      ),
      getSystemStatus: api.getSystemStatus,
      listRecipes: localCached("listRecipes", 20_000, api.listRecipes),
      connectDockerInstance: api.connectDockerInstance,
      listInstallMethods: localCached(
        "installListMethods",
        20_000,
        api.installListMethods,
      ),
      installCreateSession: api.installCreateSession,
      installGetSession: api.installGetSession,
      installDecideTarget: api.installDecideTarget,
      installOrchestratorNext: api.installOrchestratorNext,
      installRunStep: api.installRunStep,

      // SSH management (infrastructure, not abstracted)
      listSshHosts: api.listSshHosts,
      upsertSshHost: api.upsertSshHost,
      deleteSshHost: api.deleteSshHost,
      sshConnect: api.sshConnect,
      sshDisconnect: api.sshDisconnect,
      sshStatus: api.sshStatus,
      diagnoseSsh: api.diagnoseSsh,
      getSshTransferStats: api.getSshTransferStats,

      // Remote-only
      remoteWriteRawConfig: withInvalidation(api.remoteWriteRawConfig),
    }),
    [
      dispatch,
      dispatchCached,
      localCached,
      localGlobalCached,
      withInvalidation,
      withGlobalInvalidation,
      pinOptimistic,
      pinOptimisticGlobal,
      instanceId,
      instanceViewToken,
      instanceCacheKey,
      persistenceScope,
      persistenceResolved,
      isRemote,
      isDocker,
      isConnected,
      channelNodes,
      discordGuildChannels,
      channelsLoading,
      discordChannelsLoading,
      refreshChannelNodesCache,
      refreshDiscordChannelsCache,
    ],
  );
}
