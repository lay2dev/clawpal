import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "@/lib/api";
import { logDevIgnoredError } from "@/lib/dev-logging";
import {
  loadInitialSharedModelProfiles,
  shouldWarmSharedModelProfiles,
} from "@/lib/model-profile-cache";
import { shouldEnableInstanceLiveReads } from "@/lib/instance-availability";
import { writePersistedReadCache } from "@/lib/persistent-read-cache";
import type { Route } from "@/lib/routes";
import type { ModelProfile } from "@/lib/types";

interface UseModelProfileCacheParams {
  activeInstance: string;
  route: Route;
  instanceToken: number;
  persistenceScope: string | null;
  persistenceResolved: boolean;
  isRemote: boolean;
  isConnected: boolean;
}

export function useModelProfileCache(params: UseModelProfileCacheParams) {
  const {
    activeInstance,
    route,
    instanceToken,
    persistenceScope,
    persistenceResolved,
    isRemote,
    isConnected,
  } = params;

  const [modelProfilesByInstance, setModelProfilesByInstance] = useState<Record<string, ModelProfile[] | null>>(
    () => ({
      [activeInstance]: persistenceResolved
        ? loadInitialSharedModelProfiles({
            instanceId: activeInstance,
            instanceToken,
            persistenceScope,
            isRemote,
          })
        : null,
    }),
  );
  const [modelProfilesLoadingByInstance, setModelProfilesLoadingByInstance] = useState<Record<string, boolean>>({});

  const modelProfiles = useMemo(
    () => modelProfilesByInstance[activeInstance] ?? null,
    [activeInstance, modelProfilesByInstance],
  );
  const modelProfilesLoading = modelProfilesLoadingByInstance[activeInstance] ?? false;

  useEffect(() => {
    if (!persistenceResolved) {
      return;
    }

    const initialProfiles = loadInitialSharedModelProfiles({
      instanceId: activeInstance,
      instanceToken,
      persistenceScope,
      isRemote,
    });
    setModelProfilesByInstance((current) => {
      const existing = current[activeInstance];
      if (existing !== undefined && !(existing === null && initialProfiles !== null)) {
        return current;
      }
      return {
        ...current,
        [activeInstance]: initialProfiles,
      };
    });
  }, [activeInstance, instanceToken, persistenceResolved, persistenceScope, isRemote]);

  const refreshModelProfilesCache = useCallback(async () => {
    setModelProfilesLoadingByInstance((current) => ({
      ...current,
      [activeInstance]: true,
    }));
    try {
      const nextProfiles = isRemote
        ? await api.remoteListModelProfiles(activeInstance)
        : await api.listModelProfiles();
      const enabledProfiles = nextProfiles.filter((profile) => profile.enabled);

      setModelProfilesByInstance((current) => ({
        ...current,
        [activeInstance]: enabledProfiles,
      }));
      if (persistenceScope) {
        writePersistedReadCache(persistenceScope, "listRecipeModelProfiles", [], nextProfiles);
        if (!isRemote) {
          writePersistedReadCache(persistenceScope, "listModelProfiles", [], nextProfiles);
        }
      }
      return enabledProfiles;
    } finally {
      setModelProfilesLoadingByInstance((current) => ({
        ...current,
        [activeInstance]: false,
      }));
    }
  }, [activeInstance, isRemote, persistenceScope]);

  useEffect(() => {
    if (!persistenceResolved) return;
    if (isRemote && !isConnected) return;
    if (!shouldEnableInstanceLiveReads({
      instanceToken,
      persistenceResolved,
      persistenceScope,
      isRemote,
    })) return;
    if (!shouldWarmSharedModelProfiles(route)) return;
    if (modelProfiles !== null) return;

    void refreshModelProfilesCache().catch((error) => {
      logDevIgnoredError("refreshModelProfilesCache", error);
    });
  }, [
    instanceToken,
    isConnected,
    isRemote,
    modelProfiles,
    persistenceResolved,
    persistenceScope,
    refreshModelProfilesCache,
    route,
  ]);

  return {
    modelProfiles,
    modelProfilesLoading,
    refreshModelProfilesCache,
  };
}
