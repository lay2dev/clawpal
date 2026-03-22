import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import { shouldEnableInstanceLiveReads } from "@/lib/instance-availability";
import { readPersistedReadCache, writePersistedReadCache } from "@/lib/persistent-read-cache";
import { logDevIgnoredError } from "@/lib/dev-logging";
import {
  areDiscordGuildChannelsFullyResolved,
  deriveDiscordGuildChannelsFromChannelNodes,
  loadInitialSharedChannels,
  mergeDiscordGuildChannels,
  shouldWarmSharedChannels,
} from "@/lib/channel-cache";
import type { ChannelNode, DiscordGuildChannel } from "@/lib/types";
import type { Route } from "@/lib/routes";

// localStorage key and TTL for tracking when we last ran the slow Discord refresh path.
// Prevents re-running the expensive SSH + REST resolution on every page reload.
const DISCORD_REFRESH_LS_KEY = "clawpal:discord-slow-refresh";
const DISCORD_REFRESH_TTL_MS = 7 * 24 * 60 * 60 * 1000; // 1 week

function isDiscordSlowRefreshFresh(instanceId: string): boolean {
  try {
    const raw = localStorage.getItem(DISCORD_REFRESH_LS_KEY);
    if (!raw) return false;
    const map = JSON.parse(raw) as Record<string, number>;
    const ts = map[instanceId];
    return typeof ts === "number" && Date.now() - ts < DISCORD_REFRESH_TTL_MS;
  } catch {
    return false;
  }
}

function markDiscordSlowRefreshDone(instanceId: string): void {
  try {
    const raw = localStorage.getItem(DISCORD_REFRESH_LS_KEY);
    const map: Record<string, number> = raw ? (JSON.parse(raw) as Record<string, number>) : {};
    map[instanceId] = Date.now();
    localStorage.setItem(DISCORD_REFRESH_LS_KEY, JSON.stringify(map));
  } catch {}
}

function invalidateDiscordSlowRefresh(instanceId: string): void {
  try {
    const raw = localStorage.getItem(DISCORD_REFRESH_LS_KEY);
    if (!raw) return;
    const map = JSON.parse(raw) as Record<string, number>;
    delete map[instanceId];
    localStorage.setItem(DISCORD_REFRESH_LS_KEY, JSON.stringify(map));
  } catch {}
}

interface UseChannelCacheParams {
  activeInstance: string;
  route: Route;
  instanceToken: number;
  persistenceScope: string | null;
  persistenceResolved: boolean;
  isRemote: boolean;
  isConnected: boolean;
}

export function useChannelCache(params: UseChannelCacheParams) {
  const {
    activeInstance,
    route,
    instanceToken,
    persistenceScope,
    persistenceResolved,
    isRemote,
    isConnected,
  } = params;

  const [channelNodesByInstance, setChannelNodesByInstance] = useState<Record<string, ChannelNode[] | null>>(
    () => ({
      [activeInstance]: persistenceResolved
        ? loadInitialSharedChannels({
            instanceId: activeInstance,
            instanceToken,
            persistenceScope,
          }).channelNodes
        : null,
    }),
  );
  const [discordGuildChannelsByInstance, setDiscordGuildChannelsByInstance] = useState<Record<string, DiscordGuildChannel[] | null>>(
    () => ({
      [activeInstance]: persistenceResolved
        ? loadInitialSharedChannels({
            instanceId: activeInstance,
            instanceToken,
            persistenceScope,
          }).discordGuildChannels
        : null,
    }),
  );
  const [channelsLoadingByInstance, setChannelsLoadingByInstance] = useState<Record<string, boolean>>({});
  const [discordChannelsLoadingByInstance, setDiscordChannelsLoadingByInstance] = useState<Record<string, boolean>>({});
  const [discordChannelsResolvedByInstance, setDiscordChannelsResolvedByInstance] = useState<Record<string, boolean>>(
    () => ({
      [activeInstance]: persistenceResolved
        ? loadInitialSharedChannels({
            instanceId: activeInstance,
            instanceToken,
            persistenceScope,
          }).discordChannelsResolved
        : false,
    }),
  );

  // Keep a ref so callbacks can read the latest channelNodesByInstance without
  // adding it to their useCallback deps (which would cause the useEffect to
  // re-run every time channel nodes are loaded, creating an infinite loop).
  const channelNodesByInstanceRef = useRef(channelNodesByInstance);
  useEffect(() => {
    channelNodesByInstanceRef.current = channelNodesByInstance;
  });

  const channelNodes = channelNodesByInstance[activeInstance] ?? null;
  const discordGuildChannels = discordGuildChannelsByInstance[activeInstance] ?? null;
  const channelsLoading = channelsLoadingByInstance[activeInstance] ?? false;
  const discordChannelsLoading = discordChannelsLoadingByInstance[activeInstance] ?? false;
  const discordChannelsResolved = discordChannelsResolvedByInstance[activeInstance] ?? false;

  // useLayoutEffect fires synchronously before the browser paints, so state maps
  // are populated in the same frame as the instance switch — no blank flash.
  useLayoutEffect(() => {
    if (!persistenceResolved) {
      return;
    }

    const initialState = loadInitialSharedChannels({
      instanceId: activeInstance,
      instanceToken,
      persistenceScope,
    });
    setChannelNodesByInstance((current) => {
      const existing = current[activeInstance];
      if (existing !== undefined && !(existing === null && initialState.channelNodes !== null)) {
        return current;
      }
      return {
        ...current,
        [activeInstance]: initialState.channelNodes,
      };
    });
    setDiscordGuildChannelsByInstance((current) => {
      const existing = current[activeInstance];
      if (existing !== undefined && !(existing === null && initialState.discordGuildChannels !== null)) {
        return current;
      }
      return {
        ...current,
        [activeInstance]: initialState.discordGuildChannels,
      };
    });
    setDiscordChannelsResolvedByInstance((current) => {
      if (current[activeInstance] !== undefined) {
        return current;
      }
      return {
        ...current,
        [activeInstance]: initialState.discordChannelsResolved,
      };
    });
  }, [activeInstance, instanceToken, persistenceResolved, persistenceScope]);

  const refreshChannelNodesCache = useCallback(async () => {
    setChannelsLoadingByInstance((current) => ({
      ...current,
      [activeInstance]: true,
    }));
    try {
      const nodes = isRemote
        ? await api.remoteListChannelsMinimal(activeInstance)
        : await api.listChannelsMinimal();
      setChannelNodesByInstance((current) => ({
        ...current,
        [activeInstance]: nodes,
      }));
      const derivedDiscordChannels = deriveDiscordGuildChannelsFromChannelNodes(nodes);
      setDiscordGuildChannelsByInstance((current) => ({
        ...current,
        [activeInstance]: mergeDiscordGuildChannels(
          derivedDiscordChannels,
          current[activeInstance] ?? null,
        ),
      }));
      if (persistenceScope) {
        writePersistedReadCache(persistenceScope, "listChannelsMinimal", [], nodes);
      }
      return nodes;
    } finally {
      setChannelsLoadingByInstance((current) => ({
        ...current,
        [activeInstance]: false,
      }));
    }
  }, [activeInstance, isRemote, persistenceScope]);

  const refreshDiscordChannelsCache = useCallback(async (force = false) => {
    if (force) {
      // Invalidate the frontend TTL record so the next auto-warm also re-fetches.
      invalidateDiscordSlowRefresh(activeInstance);
    }
    setDiscordChannelsLoadingByInstance((current) => ({
      ...current,
      [activeInstance]: true,
    }));
    try {
      const channels = isRemote
        ? await api.remoteListDiscordGuildChannels(activeInstance, force)
        : await api.listDiscordGuildChannels();
      console.log("[useChannelCache] slow path raw channels:", channels);
      const mergedChannels = mergeDiscordGuildChannels(
        deriveDiscordGuildChannelsFromChannelNodes(channelNodesByInstanceRef.current[activeInstance] ?? []),
        channels,
      );
      const resolved = areDiscordGuildChannelsFullyResolved(mergedChannels);
      console.log("[useChannelCache] slow path merged:", mergedChannels, "resolved:", resolved);
      if (!resolved && mergedChannels) {
        const unresolved = mergedChannels.filter(
          (ch) => ch.guildName === ch.guildId || ch.channelName === ch.channelId,
        );
        console.log("[useChannelCache] unresolved channels (name === id):", unresolved);
      }
      setDiscordGuildChannelsByInstance((current) => ({
        ...current,
        [activeInstance]: mergedChannels,
      }));
      setDiscordChannelsResolvedByInstance((current) => ({
        ...current,
        [activeInstance]: resolved,
      }));
      if (persistenceScope) {
        writePersistedReadCache(persistenceScope, "listDiscordGuildChannels", [], channels);
      }
      markDiscordSlowRefreshDone(activeInstance);
      return mergedChannels ?? [];
    } finally {
      setDiscordChannelsLoadingByInstance((current) => ({
        ...current,
        [activeInstance]: false,
      }));
    }
  }, [activeInstance, isRemote, persistenceScope]);

  const refreshDiscordChannelsCacheFast = useCallback(async () => {
    try {
      const channels = isRemote
        ? await api.remoteListDiscordGuildChannelsFast(activeInstance)
        : await api.listDiscordGuildChannelsFast();
      console.log("[useChannelCache] fast path raw channels:", channels);
      const baseChannels = mergeDiscordGuildChannels(
        deriveDiscordGuildChannelsFromChannelNodes(channelNodesByInstanceRef.current[activeInstance] ?? []),
        channels,
      );
      // Only update state if we have actual data — avoid overwriting null (loading)
      // with an empty array that would prematurely show "no channels" while the
      // full refresh is still in flight.
      if (baseChannels && baseChannels.length > 0) {
        setDiscordGuildChannelsByInstance((current) => ({
          ...current,
          [activeInstance]: mergeDiscordGuildChannels(
            baseChannels,
            current[activeInstance] ?? null,
          ),
        }));
      } else {
        console.log("[useChannelCache] fast path skipped state update — baseChannels empty:", baseChannels);
      }
      return channels;
    } catch (error) {
      logDevIgnoredError("refreshDiscordChannelsCacheFast", error);
      return [];
    }
  }, [activeInstance, isRemote]);

  useEffect(() => {
    if (!shouldWarmSharedChannels(route) || !persistenceResolved) return;
    if (isRemote && !isConnected) return;
    if (!shouldEnableInstanceLiveReads({
      instanceToken,
      persistenceResolved,
      persistenceScope,
      isRemote,
    })) return;
    void refreshDiscordChannelsCacheFast();
    void refreshChannelNodesCache();
    // Skip the expensive slow path if data was refreshed within the TTL window.
    // The Rust side also has its own TTL gate, but skipping the call entirely
    // avoids even the SFTP round-trip on remote instances.
    if (!isDiscordSlowRefreshFresh(activeInstance)) {
      void refreshDiscordChannelsCache();
    }
  }, [
    route,
    instanceToken,
    persistenceResolved,
    persistenceScope,
    isRemote,
    isConnected,
    activeInstance,
    refreshChannelNodesCache,
    refreshDiscordChannelsCache,
    refreshDiscordChannelsCacheFast,
  ]);

  return {
    channelNodes,
    discordGuildChannels,
    channelsLoading,
    discordChannelsLoading,
    discordChannelsResolved,
    refreshChannelNodesCache,
    refreshDiscordChannelsCache,
    refreshDiscordChannelsCacheFast,
  };
}
