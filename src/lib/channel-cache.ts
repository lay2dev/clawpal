import { readPersistedReadCache } from "./persistent-read-cache";
import {
  buildCacheKey,
  readCacheValue,
  resolveReadCacheScopeKey,
} from "./api-read-cache";
import type {
  ChannelNode,
  ChannelsConfigSnapshot,
  ChannelsRuntimeSnapshot,
  DiscordGuildChannel,
} from "./types";
import type { Route } from "./routes";

const DISCORD_CHANNEL_PATH =
  /^channels\.discord(?:\.accounts\.[^.]+)?\.guilds\.([^.]+)\.channels\.([^.]+)(?:\.|$)/;

function isResolvedName(name: string, fallbackId: string): boolean {
  const value = name.trim();
  return value.length > 0 && value !== fallbackId;
}

function pickRicherName(current: string, incoming: string, fallbackId: string): string {
  if (isResolvedName(incoming, fallbackId) && !isResolvedName(current, fallbackId)) {
    return incoming;
  }
  if (current.trim().length === 0 && incoming.trim().length > 0) {
    return incoming;
  }
  return current;
}

export function deriveDiscordGuildChannelsFromChannelNodes(
  channelNodes: ChannelNode[],
): DiscordGuildChannel[] {
  const channels = new Map<string, DiscordGuildChannel>();

  for (const node of channelNodes) {
    const match = DISCORD_CHANNEL_PATH.exec(node.path);
    if (!match) continue;

    const [, guildId, channelId] = match;
    const key = `${guildId}:${channelId}`;
    if (channels.has(key)) continue;

    channels.set(key, {
      guildId,
      guildName: guildId,
      channelId,
      channelName: channelId,
      defaultAgentId: undefined,
      resolutionWarning: undefined,
    });
  }

  return Array.from(channels.values());
}

export function mergeDiscordGuildChannels(
  current: DiscordGuildChannel[] | null,
  incoming: DiscordGuildChannel[] | null,
): DiscordGuildChannel[] | null {
  if (!current || current.length === 0) {
    return incoming && incoming.length > 0 ? [...incoming] : current;
  }
  if (!incoming || incoming.length === 0) {
    return [...current];
  }

  const merged = new Map<string, DiscordGuildChannel>();

  for (const channel of current) {
    merged.set(`${channel.guildId}:${channel.channelId}`, { ...channel });
  }

  for (const channel of incoming) {
    const key = `${channel.guildId}:${channel.channelId}`;
    const existing = merged.get(key);
    if (!existing) {
      merged.set(key, { ...channel });
      continue;
    }

    const incomingResolved =
      isResolvedName(channel.guildName, channel.guildId)
      && isResolvedName(channel.channelName, channel.channelId);

    const mergedGuildName = pickRicherName(existing.guildName, channel.guildName, existing.guildId);
    const mergedChannelName = pickRicherName(existing.channelName, channel.channelName, existing.channelId);
    // Clear warning if the merged result has resolved names (e.g. from cache),
    // even if the backend reported a warning because the network call failed.
    const mergedFullyResolved =
      isResolvedName(mergedGuildName, existing.guildId)
      && isResolvedName(mergedChannelName, existing.channelId);

    merged.set(key, {
      ...existing,
      guildName: mergedGuildName,
      channelName: mergedChannelName,
      defaultAgentId: channel.defaultAgentId ?? existing.defaultAgentId,
      resolutionWarning: mergedFullyResolved
        ? undefined
        : (channel.resolutionWarning
          ?? (incomingResolved ? undefined : existing.resolutionWarning)),
      guildResolutionWarning: isResolvedName(mergedGuildName, existing.guildId)
        ? undefined
        : (channel.guildResolutionWarning ?? existing.guildResolutionWarning),
      channelResolutionWarning: isResolvedName(mergedChannelName, existing.channelId)
        ? undefined
        : (channel.channelResolutionWarning ?? existing.channelResolutionWarning),
    });
  }

  return Array.from(merged.values());
}

export function areDiscordGuildChannelsFullyResolved(
  channels: DiscordGuildChannel[] | null,
): boolean {
  if (!channels || channels.length === 0) {
    return false;
  }
  return channels.every((channel) =>
    isResolvedName(channel.guildName, channel.guildId)
    && isResolvedName(channel.channelName, channel.channelId),
  );
}

export function pickInitialSharedDiscordGuildChannels({
  runtimeChannels,
  configChannels,
  cachedChannels,
  fastChannels,
}: {
  runtimeChannels: ChannelNode[] | null;
  configChannels: ChannelNode[] | null;
  cachedChannels: DiscordGuildChannel[] | null;
  fastChannels: DiscordGuildChannel[] | null;
}): DiscordGuildChannel[] | null {
  const snapshotChannels = deriveDiscordGuildChannelsFromChannelNodes(
    runtimeChannels ?? configChannels ?? [],
  );
  const withFast = mergeDiscordGuildChannels(snapshotChannels, fastChannels);
  const withCached = mergeDiscordGuildChannels(withFast, cachedChannels);
  return withCached && withCached.length > 0 ? withCached : null;
}

function readSharedCachedValue<T>(
  instanceCacheKey: string,
  persistenceScope: string | null,
  method: string,
): T | null {
  const scope = resolveReadCacheScopeKey(instanceCacheKey, persistenceScope, method);
  const cached =
    readCacheValue<T>(buildCacheKey(scope, method, []))
    ?? (persistenceScope ? readPersistedReadCache<T>(persistenceScope, method, []) : undefined);
  return cached ?? null;
}

export function loadInitialSharedChannels({
  instanceId,
  instanceToken,
  persistenceScope,
}: {
  instanceId: string;
  instanceToken: number;
  persistenceScope: string | null;
}): {
  channelNodes: ChannelNode[] | null;
  discordGuildChannels: DiscordGuildChannel[] | null;
  discordChannelsResolved: boolean;
} {
  const instanceCacheKey = `${instanceId}#${instanceToken}`;
  const runtimeSnapshot = readSharedCachedValue<ChannelsRuntimeSnapshot>(
    instanceCacheKey,
    persistenceScope,
    "getChannelsRuntimeSnapshot",
  );
  const configSnapshot = readSharedCachedValue<ChannelsConfigSnapshot>(
    instanceCacheKey,
    persistenceScope,
    "getChannelsConfigSnapshot",
  );
  const cachedChannelNodes = readSharedCachedValue<ChannelNode[]>(
    instanceCacheKey,
    persistenceScope,
    "listChannelsMinimal",
  );
  const cachedDiscordChannels = readSharedCachedValue<DiscordGuildChannel[]>(
    instanceCacheKey,
    persistenceScope,
    "listDiscordGuildChannels",
  );
  const fastDiscordChannels = readSharedCachedValue<DiscordGuildChannel[]>(
    instanceCacheKey,
    persistenceScope,
    "listDiscordGuildChannelsFast",
  );

  const channelNodes =
    runtimeSnapshot?.channels
    ?? configSnapshot?.channels
    ?? cachedChannelNodes
    ?? null;
  const discordGuildChannels = pickInitialSharedDiscordGuildChannels({
    runtimeChannels: runtimeSnapshot?.channels ?? channelNodes,
    configChannels: configSnapshot?.channels ?? channelNodes,
    cachedChannels: cachedDiscordChannels,
    fastChannels: fastDiscordChannels,
  });

  return {
    channelNodes,
    discordGuildChannels,
    discordChannelsResolved: areDiscordGuildChannelsFullyResolved(cachedDiscordChannels),
  };
}

export function shouldWarmSharedChannels(route: Route): boolean {
  return (
    route === "channels"
    || route === "cook"
    || route === "recipes"
    || route === "recipe-studio"
    || route === "cron"
  );
}
