import type {
  AgentOverview,
  ChannelsConfigSnapshot,
  ChannelsRuntimeSnapshot,
  CronConfigSnapshot,
  CronRuntimeSnapshot,
  InstanceConfigSnapshot,
  InstanceRuntimeSnapshot,
  InstanceStatus,
  RescueBotManageResult,
  StatusExtra,
} from "@/lib/types";

export function buildInstanceCardSummary(
  configSnapshot: { agents?: { id: string }[] } | null,
  runtimeSnapshot: { status: { healthy: boolean | null; activeAgents: number } } | null,
): { healthy: boolean | null; agentCount: number } {
  if (runtimeSnapshot) {
    return {
      healthy: runtimeSnapshot.status.healthy,
      agentCount: runtimeSnapshot.status.activeAgents,
    };
  }

  return {
    healthy: null,
    agentCount: configSnapshot?.agents?.length ?? 0,
  };
}

export function shouldStartDeferredUpdateCheck({
  isRemote,
  isConnected,
}: {
  isRemote: boolean;
  isConnected: boolean;
}): boolean {
  if (isRemote && !isConnected) return false;
  return true;
}

export function buildInitialHomeState(
  configSnapshot: InstanceConfigSnapshot | null,
  runtimeSnapshot: InstanceRuntimeSnapshot | null,
  statusExtra: StatusExtra | null,
): {
  status: InstanceStatus | null;
  agents: AgentOverview[] | null;
  statusSettled: boolean;
  version: string | null;
  statusExtra: StatusExtra | null;
} {
  if (runtimeSnapshot) {
    return {
      status: {
        ...runtimeSnapshot.status,
        globalDefaultModel: runtimeSnapshot.globalDefaultModel,
        fallbackModels: runtimeSnapshot.fallbackModels,
      },
      agents: runtimeSnapshot.agents,
      statusSettled: true,
      version: statusExtra?.openclawVersion ?? null,
      statusExtra,
    };
  }

  if (configSnapshot) {
    return {
      status: {
        healthy: null,
        activeAgents: configSnapshot.agents.length,
        globalDefaultModel: configSnapshot.globalDefaultModel,
        fallbackModels: configSnapshot.fallbackModels,
        sshDiagnostic: null,
      },
      agents: configSnapshot.agents,
      statusSettled: false,
      version: statusExtra?.openclawVersion ?? null,
      statusExtra,
    };
  }

  return {
    status: null,
    agents: null,
    statusSettled: false,
    version: statusExtra?.openclawVersion ?? null,
    statusExtra,
  };
}

export function applyConfigSnapshotToHomeState(
  current: {
    status: InstanceStatus | null;
    agents: AgentOverview[] | null;
    statusSettled: boolean;
    version: string | null;
    statusExtra: StatusExtra | null;
  },
  snapshot: {
    globalDefaultModel?: string;
    fallbackModels: string[];
    agents: AgentOverview[];
  },
): {
  status: InstanceStatus | null;
  agents: AgentOverview[] | null;
  statusSettled: boolean;
  version: string | null;
  statusExtra: StatusExtra | null;
} {
  if (current.statusSettled && current.status && current.agents) {
    return current;
  }

  return {
    status: {
      healthy: null,
      activeAgents: snapshot.agents.length,
      globalDefaultModel: snapshot.globalDefaultModel,
      fallbackModels: snapshot.fallbackModels,
      sshDiagnostic: null,
    },
    agents: snapshot.agents,
    statusSettled: false,
    version: current.version,
    statusExtra: current.statusExtra,
  };
}

export function buildInitialChannelsState(
  configSnapshot: ChannelsConfigSnapshot | null,
  runtimeSnapshot: ChannelsRuntimeSnapshot | null,
): {
  channels: ChannelsRuntimeSnapshot["channels"];
  bindings: ChannelsRuntimeSnapshot["bindings"];
  agents: ChannelsRuntimeSnapshot["agents"];
  loaded: boolean;
} {
  if (runtimeSnapshot) {
    return {
      channels: runtimeSnapshot.channels,
      bindings: runtimeSnapshot.bindings,
      agents: runtimeSnapshot.agents,
      loaded: true,
    };
  }

  if (configSnapshot) {
    return {
      channels: configSnapshot.channels,
      bindings: configSnapshot.bindings,
      agents: [],
      loaded: true,
    };
  }

  return {
    channels: [],
    bindings: [],
    agents: [],
    loaded: false,
  };
}

export function buildInitialCronState(
  configSnapshot: CronConfigSnapshot | null,
  runtimeSnapshot: CronRuntimeSnapshot | null,
): {
  jobs: CronRuntimeSnapshot["jobs"];
  watchdog: CronRuntimeSnapshot["watchdog"] | null;
} {
  if (runtimeSnapshot) {
    return {
      jobs: runtimeSnapshot.jobs,
      watchdog: runtimeSnapshot.watchdog,
    };
  }

  return {
    jobs: configSnapshot?.jobs ?? [],
    watchdog: null,
  };
}

export function buildInitialRescueState(
  persistedStatus: RescueBotManageResult | null,
): {
  runtimeState: RescueBotManageResult["runtimeState"];
  configured: boolean;
  active: boolean;
  profile: string;
  port: number | null;
} | null {
  if (!persistedStatus) {
    return null;
  }
  return {
    runtimeState: persistedStatus.runtimeState,
    configured: persistedStatus.configured,
    active: persistedStatus.active,
    profile: persistedStatus.profile,
    port: persistedStatus.configured ? persistedStatus.rescuePort : null,
  };
}

export function shouldShowAvailableUpdateBadge({
  checkingUpdate,
  updateInfo,
  version,
}: {
  checkingUpdate: boolean;
  updateInfo: { available: boolean; latest?: string } | null;
  version: string | null;
}): boolean {
  return Boolean(
    !checkingUpdate
      && updateInfo?.available
      && updateInfo.latest
      && updateInfo.latest !== version,
  );
}

export function shouldShowLatestReleaseBadge({
  checkingUpdate,
  updateInfo,
  version,
}: {
  checkingUpdate: boolean;
  updateInfo: { available: boolean; latest?: string } | null;
  version: string | null;
}): boolean {
  if (checkingUpdate || !updateInfo?.latest) return false;
  return !shouldShowAvailableUpdateBadge({
    checkingUpdate,
    updateInfo,
    version,
  });
}


// ---------------------------------------------------------------------------
// P0: Skip redundant ConfigSnapshot when RuntimeSnapshot is already cached
// ---------------------------------------------------------------------------

export function shouldSkipConfigSnapshot(
  persistedRuntimeSnapshot: { globalDefaultModel?: string | null } | null,
): boolean {
  if (persistedRuntimeSnapshot == null) return false;
  // Only skip if the cached runtime snapshot actually has model data.
  // The remote SSH path had a bug where globalDefaultModel was always null
  // due to a JSON pointer mismatch, so we must not skip ConfigSnapshot
  // when the cached data is incomplete.
  return persistedRuntimeSnapshot.globalDefaultModel != null;
}

// ---------------------------------------------------------------------------
// P0: Unified poll interval computation
// ---------------------------------------------------------------------------

export type HomePollContext = {
  isRemote: boolean;
  statusSettled: boolean;
};

/**
 * Compute the poll interval in ms for the unified Home data refresh loop.
 * - Remote instances always poll slowly (30s) to avoid SSH overhead.
 * - Local unsettled: fast-poll (2s) until health resolves.
 * - Local settled: slow-poll (10s).
 */
export function computePollIntervalMs(ctx: HomePollContext): number {
  if (ctx.isRemote) return 30_000;
  return ctx.statusSettled ? 10_000 : 2_000;
}

export type PollResource =
  | "runtimeSnapshot"
  | "queuedCommandsCount"
  | "statusExtra";

/**
 * Decide which resources to refresh on a given poll tick.
 * - `runtimeSnapshot` and `queuedCommandsCount`: every tick.
 * - `statusExtra`: every 3rd tick (it changes rarely).
 */
export function shouldPollResource(
  resource: PollResource,
  tick: number,
): boolean {
  switch (resource) {
    case "runtimeSnapshot":
    case "queuedCommandsCount":
      return true;
    case "statusExtra":
      return tick % 3 === 0;
  }
}
