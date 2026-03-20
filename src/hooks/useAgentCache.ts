import { useCallback, useEffect, useMemo, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { api } from "@/lib/api";
import { logDevIgnoredError } from "@/lib/dev-logging";
import { loadInitialSharedAgents, shouldWarmSharedAgents } from "@/lib/agent-cache";
import { shouldEnableInstanceLiveReads } from "@/lib/instance-availability";
import type { Route } from "@/lib/routes";
import type { AgentOverview } from "@/lib/types";

interface UseAgentCacheParams {
  activeInstance: string;
  route: Route;
  chatOpen: boolean;
  instanceToken: number;
  persistenceScope: string | null;
  persistenceResolved: boolean;
  isRemote: boolean;
  isConnected: boolean;
}

export function useAgentCache(params: UseAgentCacheParams) {
  const {
    activeInstance,
    route,
    chatOpen,
    instanceToken,
    persistenceScope,
    persistenceResolved,
    isRemote,
    isConnected,
  } = params;

  const [agentsByInstance, setAgentsByInstance] = useState<Record<string, AgentOverview[] | null>>(
    () => ({
      [activeInstance]: persistenceResolved
        ? loadInitialSharedAgents({
            instanceId: activeInstance,
            instanceToken,
            persistenceScope,
          })
        : null,
    }),
  );
  const [agentsLoading, setAgentsLoading] = useState(false);

  const agents = useMemo(
    () => agentsByInstance[activeInstance] ?? null,
    [activeInstance, agentsByInstance],
  );

  useEffect(() => {
    if (!persistenceResolved) {
      return;
    }
    const initialAgents = loadInitialSharedAgents({
      instanceId: activeInstance,
      instanceToken,
      persistenceScope,
    });
    setAgentsByInstance((current) => {
      const existing = current[activeInstance];
      if (existing !== undefined && existing !== null) {
        return current;
      }
      if (existing === initialAgents) {
        return current;
      }
      return {
        ...current,
        [activeInstance]: initialAgents,
      };
    });
  }, [activeInstance, instanceToken, persistenceResolved, persistenceScope]);

  const setAgentsCache = useCallback<Dispatch<SetStateAction<AgentOverview[] | null>>>(
    (next) => {
      setAgentsByInstance((current) => ({
        ...current,
        [activeInstance]:
          typeof next === "function"
            ? next(current[activeInstance] ?? null)
            : next,
      }));
    },
    [activeInstance],
  );

  const refreshAgentsCache = useCallback(async () => {
    setAgentsLoading(true);
    try {
      const nextAgents = isRemote
        ? await api.remoteListAgentsOverview(activeInstance)
        : await api.listAgentsOverview();
      setAgentsByInstance((current) => ({
        ...current,
        [activeInstance]: nextAgents,
      }));
      return nextAgents;
    } finally {
      setAgentsLoading(false);
    }
  }, [activeInstance, isRemote]);

  useEffect(() => {
    if (!persistenceResolved) return;
    if (isRemote && !isConnected) return;
    if (!shouldEnableInstanceLiveReads({
      instanceToken,
      persistenceResolved,
      persistenceScope,
      isRemote,
    })) return;
    if (!shouldWarmSharedAgents(route, chatOpen)) return;
    if (agents !== null) return;
    void refreshAgentsCache().catch((error) => {
      logDevIgnoredError("refreshAgentsCache", error);
    });
  }, [
    agents,
    chatOpen,
    instanceToken,
    isConnected,
    isRemote,
    persistenceResolved,
    persistenceScope,
    refreshAgentsCache,
    route,
  ]);

  return {
    agents,
    agentsLoading,
    setAgentsCache,
    refreshAgentsCache,
  };
}
