import { readPersistedReadCache } from "./persistent-read-cache";
import {
  buildCacheKey,
  readCacheValue,
  resolveReadCacheScopeKey,
} from "./api-read-cache";
import type { ModelProfile } from "./types";
import type { Route } from "./routes";

function filterEnabledModelProfiles(
  profiles: ModelProfile[] | null,
): ModelProfile[] | null {
  if (!profiles) {
    return null;
  }
  return profiles.filter((profile) => profile.enabled);
}

export function pickInitialSharedModelProfiles({
  cachedProfiles,
  persistedProfiles,
}: {
  cachedProfiles: ModelProfile[] | null;
  persistedProfiles: ModelProfile[] | null;
}): ModelProfile[] | null {
  const cachedEnabled = filterEnabledModelProfiles(cachedProfiles);
  if (cachedEnabled && cachedEnabled.length > 0) {
    return cachedEnabled;
  }

  const persistedEnabled = filterEnabledModelProfiles(persistedProfiles);
  if (persistedEnabled && persistedEnabled.length > 0) {
    return persistedEnabled;
  }

  return cachedEnabled ?? persistedEnabled ?? null;
}

export function loadInitialSharedModelProfiles({
  instanceId,
  instanceToken,
  persistenceScope,
  isRemote,
}: {
  instanceId: string;
  instanceToken: number;
  persistenceScope: string | null;
  isRemote: boolean;
}): ModelProfile[] | null {
  const instanceCacheKey = `${instanceId}#${instanceToken}`;
  const recipeScope = resolveReadCacheScopeKey(
    instanceCacheKey,
    persistenceScope,
    "listRecipeModelProfiles",
  );
  const cachedRecipeProfiles =
    readCacheValue<ModelProfile[]>(
      buildCacheKey(recipeScope, "listRecipeModelProfiles", []),
    )
    ?? (persistenceScope
      ? readPersistedReadCache<ModelProfile[]>(
          persistenceScope,
          "listRecipeModelProfiles",
          [],
        )
      : undefined)
    ?? null;

  if (isRemote) {
    return pickInitialSharedModelProfiles({
      cachedProfiles: cachedRecipeProfiles,
      persistedProfiles: cachedRecipeProfiles,
    });
  }

  const cachedLocalProfiles =
    readCacheValue<ModelProfile[]>(
      buildCacheKey("__global__", "listModelProfiles", []),
    )
    ?? (persistenceScope
      ? readPersistedReadCache<ModelProfile[]>(persistenceScope, "listModelProfiles", [])
      : undefined)
    ?? null;

  return pickInitialSharedModelProfiles({
    cachedProfiles: cachedRecipeProfiles ?? cachedLocalProfiles,
    persistedProfiles: cachedLocalProfiles,
  });
}

export function shouldWarmSharedModelProfiles(route: Route): boolean {
  return route === "home" || route === "channels" || route === "cook";
}
