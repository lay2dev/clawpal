import { useCallback, useEffect, useMemo, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { api } from "@/lib/api";
import { shouldEnableInstanceLiveReads } from "@/lib/instance-availability";
import { readPersistedReadCache } from "@/lib/persistent-read-cache";
import type { Route } from "@/lib/routes";
import type {
  AgentOverview,
  AgentSessionAnalysis,
  BackupInfo,
  ChannelsConfigSnapshot,
  ChannelsRuntimeSnapshot,
  HistoryItem,
  RecipeRuntimeRun,
  SessionFile,
} from "@/lib/types";

type ChannelsPageState = {
  configSnapshot: ChannelsConfigSnapshot | null;
  runtimeSnapshot: ChannelsRuntimeSnapshot | null;
  loading: boolean;
  loaded: boolean;
};

type HistoryPageState = {
  items: HistoryItem[];
  runs: RecipeRuntimeRun[];
  loading: boolean;
  loaded: boolean;
};

type ContextPageState = {
  sessionFiles: SessionFile[];
  sessionAnalysis: AgentSessionAnalysis[] | null;
  sessionsLoading: boolean;
  sessionsLoaded: boolean;
  backups: BackupInfo[] | null;
  backupsLoading: boolean;
  backupsLoaded: boolean;
};

interface UseInstanceDataStoreParams {
  activeInstance: string;
  route: Route;
  instanceToken: number;
  persistenceScope: string | null;
  persistenceResolved: boolean;
  isRemote: boolean;
  isConnected: boolean;
  setAgentsCache: Dispatch<SetStateAction<AgentOverview[] | null>>;
  refreshChannelNodesCache: () => Promise<unknown>;
}

const EMPTY_HISTORY_STATE: HistoryPageState = {
  items: [],
  runs: [],
  loading: false,
  loaded: false,
};

const EMPTY_CONTEXT_STATE: ContextPageState = {
  sessionFiles: [],
  sessionAnalysis: null,
  sessionsLoading: false,
  sessionsLoaded: false,
  backups: null,
  backupsLoading: false,
  backupsLoaded: false,
};

export function useInstanceDataStore(params: UseInstanceDataStoreParams) {
  const {
    activeInstance,
    route,
    instanceToken,
    persistenceScope,
    persistenceResolved,
    isRemote,
    isConnected,
    setAgentsCache,
    refreshChannelNodesCache,
  } = params;

  const scopedKey = `${activeInstance}#${instanceToken}`;
  const liveReadsReady =
    shouldEnableInstanceLiveReads({
      instanceToken,
      persistenceResolved,
      persistenceScope,
      isRemote,
    }) && (!isRemote || isConnected);

  const initialChannelsPageState = useMemo<ChannelsPageState>(() => {
    if (!persistenceResolved || !persistenceScope) {
      return {
        configSnapshot: null,
        runtimeSnapshot: null,
        loading: false,
        loaded: false,
      };
    }

    return {
      configSnapshot:
        readPersistedReadCache<ChannelsConfigSnapshot>(
          persistenceScope,
          "getChannelsConfigSnapshot",
          [],
        ) ?? null,
      runtimeSnapshot:
        readPersistedReadCache<ChannelsRuntimeSnapshot>(
          persistenceScope,
          "getChannelsRuntimeSnapshot",
          [],
        ) ?? null,
      loading: false,
      loaded: false,
    };
  }, [persistenceResolved, persistenceScope, scopedKey]);

  const [channelsPageByKey, setChannelsPageByKey] = useState<Record<string, ChannelsPageState>>(
    {},
  );
  const [historyPageByKey, setHistoryPageByKey] = useState<Record<string, HistoryPageState>>({});
  const [contextPageByKey, setContextPageByKey] = useState<Record<string, ContextPageState>>({});

  const channelsPageState = channelsPageByKey[scopedKey] ?? initialChannelsPageState;
  const historyPageState = historyPageByKey[scopedKey] ?? EMPTY_HISTORY_STATE;
  const contextPageState = contextPageByKey[scopedKey] ?? EMPTY_CONTEXT_STATE;

  const refreshChannelsSnapshotState = useCallback(async () => {
    if (!liveReadsReady) {
      return;
    }

    setChannelsPageByKey((current) => ({
      ...current,
      [scopedKey]: {
        ...(current[scopedKey] ?? initialChannelsPageState),
        loading: true,
      },
    }));

    try {
      const [configSnapshot, runtimeSnapshot] = await Promise.all([
        isRemote
          ? api.remoteGetChannelsConfigSnapshot(activeInstance)
          : api.getChannelsConfigSnapshot(),
        isRemote
          ? api.remoteGetChannelsRuntimeSnapshot(activeInstance)
          : api.getChannelsRuntimeSnapshot(),
      ]);

      setChannelsPageByKey((current) => ({
        ...current,
        [scopedKey]: {
          configSnapshot,
          runtimeSnapshot,
          loading: false,
          loaded: true,
        },
      }));
      setAgentsCache(runtimeSnapshot.agents);
      void refreshChannelNodesCache().catch(() => {});
    } catch (error) {
      setChannelsPageByKey((current) => ({
        ...current,
        [scopedKey]: {
          ...(current[scopedKey] ?? initialChannelsPageState),
          loading: false,
          loaded: true,
        },
      }));
      throw error;
    }
  }, [
    activeInstance,
    initialChannelsPageState,
    isRemote,
    liveReadsReady,
    refreshChannelNodesCache,
    scopedKey,
    setAgentsCache,
  ]);

  const refreshHistoryState = useCallback(async () => {
    if (!liveReadsReady) {
      return;
    }

    setHistoryPageByKey((current) => ({
      ...current,
      [scopedKey]: {
        ...(current[scopedKey] ?? EMPTY_HISTORY_STATE),
        loading: true,
      },
    }));

    try {
      const [historyResponse, runs] = await Promise.all([
        isRemote ? api.remoteListHistory(activeInstance) : api.listHistory(),
        api.listRecipeRuns().catch(() => [] as RecipeRuntimeRun[]),
      ]);

      setHistoryPageByKey((current) => ({
        ...current,
        [scopedKey]: {
          items: historyResponse.items,
          runs,
          loading: false,
          loaded: true,
        },
      }));
    } catch (error) {
      setHistoryPageByKey((current) => ({
        ...current,
        [scopedKey]: {
          ...(current[scopedKey] ?? EMPTY_HISTORY_STATE),
          loading: false,
          loaded: true,
        },
      }));
      throw error;
    }
  }, [activeInstance, isRemote, liveReadsReady, scopedKey]);

  const refreshSessionFiles = useCallback(async () => {
    if (!liveReadsReady) {
      return [];
    }

    setContextPageByKey((current) => ({
      ...current,
      [scopedKey]: {
        ...(current[scopedKey] ?? EMPTY_CONTEXT_STATE),
        sessionsLoading: true,
      },
    }));

    try {
      const sessionFiles = isRemote
        ? await api.remoteListSessionFiles(activeInstance)
        : await api.listSessionFiles();

      setContextPageByKey((current) => ({
        ...current,
        [scopedKey]: {
          ...(current[scopedKey] ?? EMPTY_CONTEXT_STATE),
          sessionFiles,
          sessionsLoading: false,
          sessionsLoaded: true,
        },
      }));
      return sessionFiles;
    } catch (error) {
      setContextPageByKey((current) => ({
        ...current,
        [scopedKey]: {
          ...(current[scopedKey] ?? EMPTY_CONTEXT_STATE),
          sessionsLoading: false,
          sessionsLoaded: true,
        },
      }));
      throw error;
    }
  }, [activeInstance, isRemote, liveReadsReady, scopedKey]);

  const refreshBackups = useCallback(async () => {
    if (!liveReadsReady) {
      return [];
    }

    setContextPageByKey((current) => ({
      ...current,
      [scopedKey]: {
        ...(current[scopedKey] ?? EMPTY_CONTEXT_STATE),
        backupsLoading: true,
      },
    }));

    try {
      const backups = isRemote
        ? await api.remoteListBackups(activeInstance)
        : await api.listBackups();

      setContextPageByKey((current) => ({
        ...current,
        [scopedKey]: {
          ...(current[scopedKey] ?? EMPTY_CONTEXT_STATE),
          backups,
          backupsLoading: false,
          backupsLoaded: true,
        },
      }));
      return backups;
    } catch (error) {
      setContextPageByKey((current) => ({
        ...current,
        [scopedKey]: {
          ...(current[scopedKey] ?? EMPTY_CONTEXT_STATE),
          backupsLoading: false,
          backupsLoaded: true,
        },
      }));
      throw error;
    }
  }, [activeInstance, isRemote, liveReadsReady, scopedKey]);

  const setSessionAnalysis = useCallback<
    Dispatch<SetStateAction<AgentSessionAnalysis[] | null>>
  >(
    (next) => {
      setContextPageByKey((current) => {
        const existing = current[scopedKey] ?? EMPTY_CONTEXT_STATE;
        return {
          ...current,
          [scopedKey]: {
            ...existing,
            sessionAnalysis:
              typeof next === "function" ? next(existing.sessionAnalysis) : next,
          },
        };
      });
    },
    [scopedKey],
  );

  const setBackups = useCallback<Dispatch<SetStateAction<BackupInfo[] | null>>>(
    (next) => {
      setContextPageByKey((current) => {
        const existing = current[scopedKey] ?? EMPTY_CONTEXT_STATE;
        return {
          ...current,
          [scopedKey]: {
            ...existing,
            backups: typeof next === "function" ? next(existing.backups) : next,
            backupsLoaded: true,
          },
        };
      });
    },
    [scopedKey],
  );

  useEffect(() => {
    if (!liveReadsReady) {
      return;
    }

    if (
      route === "channels"
      && !channelsPageState.loaded
      && !channelsPageState.loading
    ) {
      void refreshChannelsSnapshotState().catch(() => {});
    }

    if (
      route === "history"
      && !historyPageState.loaded
      && !historyPageState.loading
    ) {
      void refreshHistoryState().catch(() => {});
    }

    if (route === "context") {
      if (
        !contextPageState.sessionsLoaded
        && !contextPageState.sessionsLoading
      ) {
        void refreshSessionFiles().catch(() => {});
      }

      if (
        !contextPageState.backupsLoaded
        && !contextPageState.backupsLoading
      ) {
        void refreshBackups().catch(() => {});
      }
    }
  }, [
    channelsPageState.loaded,
    channelsPageState.loading,
    contextPageState.backupsLoaded,
    contextPageState.backupsLoading,
    contextPageState.sessionsLoaded,
    contextPageState.sessionsLoading,
    historyPageState.loaded,
    historyPageState.loading,
    liveReadsReady,
    refreshBackups,
    refreshChannelsSnapshotState,
    refreshHistoryState,
    refreshSessionFiles,
    route,
  ]);

  return {
    channelsConfigSnapshot: channelsPageState.configSnapshot,
    channelsRuntimeSnapshot: channelsPageState.runtimeSnapshot,
    channelsSnapshotsLoading: channelsPageState.loading,
    channelsSnapshotsLoaded: channelsPageState.loaded,
    refreshChannelsSnapshotState,
    historyItems: historyPageState.items,
    historyRuns: historyPageState.runs,
    historyLoading: historyPageState.loading,
    historyLoaded: historyPageState.loaded,
    refreshHistoryState,
    sessionFiles: contextPageState.sessionFiles,
    sessionAnalysis: contextPageState.sessionAnalysis,
    sessionsLoading: contextPageState.sessionsLoading,
    sessionsLoaded: contextPageState.sessionsLoaded,
    setSessionAnalysis,
    refreshSessionFiles,
    backups: contextPageState.backups,
    backupsLoading: contextPageState.backupsLoading,
    backupsLoaded: contextPageState.backupsLoaded,
    setBackups,
    refreshBackups,
  };
}
