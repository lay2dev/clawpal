import { useCallback, useEffect, useMemo, useState } from "react";
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

  const channelNodes = useMemo(
    () => channelNodesByInstance[activeInstance] ?? null,
    [activeInstance, channelNodesByInstance],
  );
  const discordGuildChannels = useMemo(
    () => discordGuildChannelsByInstance[activeInstance] ?? null,
    [activeInstance, discordGuildChannelsByInstance],
  );
  const channelsLoading = channelsLoadingByInstance[activeInstance] ?? false;
  const discordChannelsLoading = discordChannelsLoadingByInstance[activeInstance] ?? false;
  const discordChannelsResolved = discordChannelsResolvedByInstance[activeInstance] ?? false;

  useEffect(() => {
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

  const refreshDiscordChannelsCache = useCallback(async () => {
    setDiscordChannelsLoadingByInstance((current) => ({
      ...current,
      [activeInstance]: true,
    }));
    try {
      const channels = isRemote
        ? await api.remoteListDiscordGuildChannels(activeInstance)
        : await api.listDiscordGuildChannels();
      const mergedChannels = mergeDiscordGuildChannels(
        deriveDiscordGuildChannelsFromChannelNodes(channelNodesByInstance[activeInstance] ?? []),
        channels,
      );
      setDiscordGuildChannelsByInstance((current) => ({
        ...current,
        [activeInstance]: mergedChannels,
      }));
      setDiscordChannelsResolvedByInstance((current) => ({
        ...current,
        [activeInstance]: areDiscordGuildChannelsFullyResolved(mergedChannels),
      }));
      if (persistenceScope) {
        writePersistedReadCache(persistenceScope, "listDiscordGuildChannels", [], channels);
      }
      return mergedChannels ?? [];
    } finally {
      setDiscordChannelsLoadingByInstance((current) => ({
        ...current,
        [activeInstance]: false,
      }));
    }
  }, [activeInstance, channelNodesByInstance, isRemote, persistenceScope]);

  const refreshDiscordChannelsCacheFast = useCallback(async () => {
    try {
      const channels = isRemote
        ? await api.remoteListDiscordGuildChannelsFast(activeInstance)
        : await api.listDiscordGuildChannelsFast();
      const baseChannels = mergeDiscordGuildChannels(
        deriveDiscordGuildChannelsFromChannelNodes(channelNodesByInstance[activeInstance] ?? []),
        channels,
      );
      setDiscordGuildChannelsByInstance((current) => ({
        ...current,
        [activeInstance]: mergeDiscordGuildChannels(
          baseChannels,
          current[activeInstance] ?? null,
        ),
      }));
      return channels;
    } catch (error) {
      logDevIgnoredError("refreshDiscordChannelsCacheFast", error);
      return [];
    }
  }, [activeInstance, channelNodesByInstance, isRemote]);

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
    void Promise.allSettled([
      refreshChannelNodesCache(),
      refreshDiscordChannelsCache(),
    ]);
  }, [
    route,
    instanceToken,
    persistenceResolved,
    persistenceScope,
    isRemote,
    isConnected,
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
