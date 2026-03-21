import { readPersistedReadCache } from "./persistent-read-cache";
import {
  buildCacheKey,
  readCacheValue,
  resolveReadCacheScopeKey,
} from "./api-read-cache";
import type {
  AgentOverview,
  InstanceConfigSnapshot,
  InstanceRuntimeSnapshot,
} from "./types";
import type { Route } from "./routes";

export function pickInitialSharedAgents({
  runtimeAgents,
  configAgents,
  cachedAgents,
}: {
  runtimeAgents: AgentOverview[] | null;
  configAgents: AgentOverview[] | null;
  cachedAgents: AgentOverview[] | null;
}): AgentOverview[] | null {
  if (runtimeAgents !== null) {
    return runtimeAgents;
  }
  if (configAgents !== null) {
    return configAgents;
  }
  if (cachedAgents !== null) {
    return cachedAgents;
  }
  return null;
}

export function loadInitialSharedAgents({
  instanceId,
  instanceToken,
  persistenceScope,
}: {
  instanceId: string;
  instanceToken: number;
  persistenceScope: string | null;
}): AgentOverview[] | null {
  const instanceCacheKey = `${instanceId}#${instanceToken}`;
  const runtimeScope = resolveReadCacheScopeKey(
    instanceCacheKey,
    persistenceScope,
    "getInstanceRuntimeSnapshot",
  );
  const configScope = resolveReadCacheScopeKey(
    instanceCacheKey,
    persistenceScope,
    "getInstanceConfigSnapshot",
  );

  const runtimeSnapshot =
    readCacheValue<InstanceRuntimeSnapshot>(
      buildCacheKey(runtimeScope, "getInstanceRuntimeSnapshot", []),
    ) ??
    (persistenceScope
      ? readPersistedReadCache<InstanceRuntimeSnapshot>(
          persistenceScope,
          "getInstanceRuntimeSnapshot",
          [],
        )
      : undefined);
  const configSnapshot =
    readCacheValue<InstanceConfigSnapshot>(
      buildCacheKey(configScope, "getInstanceConfigSnapshot", []),
    ) ??
    (persistenceScope
      ? readPersistedReadCache<InstanceConfigSnapshot>(
          persistenceScope,
          "getInstanceConfigSnapshot",
          [],
        )
      : undefined);
  const cachedAgents = readCacheValue<AgentOverview[]>(
    buildCacheKey(instanceCacheKey, "listAgents", []),
  );

  return pickInitialSharedAgents({
    runtimeAgents: runtimeSnapshot?.agents ?? null,
    configAgents: configSnapshot?.agents ?? null,
    cachedAgents: cachedAgents ?? null,
  });
}

export function shouldWarmSharedAgents(route: Route, chatOpen: boolean): boolean {
  if (chatOpen) {
    return true;
  }
  return route === "home" || route === "channels" || route === "cook";
}
