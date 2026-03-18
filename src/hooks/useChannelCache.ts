import { useCallback, useEffect, useState } from "react";
import { api } from "@/lib/api";
import { shouldEnableInstanceLiveReads } from "@/lib/instance-availability";
import { readPersistedReadCache, writePersistedReadCache } from "@/lib/persistent-read-cache";
import { logDevIgnoredError } from "@/lib/dev-logging";
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

  const [channelNodes, setChannelNodes] = useState<ChannelNode[] | null>(null);
  const [discordGuildChannels, setDiscordGuildChannels] = useState<DiscordGuildChannel[] | null>(null);
  const [channelsLoading, setChannelsLoading] = useState(false);
  const [discordChannelsLoading, setDiscordChannelsLoading] = useState(false);

  // Load cached channel data on instance/scope change
  useEffect(() => {
    if (!persistenceResolved || !persistenceScope) {
      setChannelNodes(null);
      setDiscordGuildChannels(null);
      return;
    }
    setChannelNodes(
      readPersistedReadCache<ChannelNode[]>(persistenceScope, "listChannelsMinimal", []) ?? null,
    );
    setDiscordGuildChannels(
      readPersistedReadCache<DiscordGuildChannel[]>(persistenceScope, "listDiscordGuildChannels", []) ?? null,
    );
  }, [activeInstance, persistenceResolved, persistenceScope]);

  const refreshChannelNodesCache = useCallback(async () => {
    setChannelsLoading(true);
    try {
      const nodes = isRemote
        ? await api.remoteListChannelsMinimal(activeInstance)
        : await api.listChannelsMinimal();
      setChannelNodes(nodes);
      if (persistenceScope) {
        writePersistedReadCache(persistenceScope, "listChannelsMinimal", [], nodes);
      }
      return nodes;
    } finally {
      setChannelsLoading(false);
    }
  }, [activeInstance, isRemote, persistenceScope]);

  const refreshDiscordChannelsCache = useCallback(async () => {
    setDiscordChannelsLoading(true);
    try {
      const channels = isRemote
        ? await api.remoteListDiscordGuildChannels(activeInstance)
        : await api.listDiscordGuildChannels();
      setDiscordGuildChannels(channels);
      if (persistenceScope) {
        writePersistedReadCache(persistenceScope, "listDiscordGuildChannels", [], channels);
      }
      return channels;
    } finally {
      setDiscordChannelsLoading(false);
    }
  }, [activeInstance, isRemote, persistenceScope]);

  // Lazy-load channel cache when Channels route is active
  useEffect(() => {
    if (route !== "channels" || !persistenceResolved) return;
    if (isRemote && !isConnected) return;
    if (!shouldEnableInstanceLiveReads({
      instanceToken,
      persistenceResolved,
      persistenceScope,
      isRemote,
    })) return;
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
  ]);

  return {
    channelNodes,
    discordGuildChannels,
    channelsLoading,
    discordChannelsLoading,
    refreshChannelNodesCache,
    refreshDiscordChannelsCache,
  };
}
