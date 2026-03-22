import { beforeEach, describe, expect, test } from "bun:test";

import {
  deriveDiscordGuildChannelsFromChannelNodes,
  loadInitialSharedChannels,
  mergeDiscordGuildChannels,
  pickInitialSharedDiscordGuildChannels,
  shouldWarmSharedChannels,
} from "../channel-cache";
import { writePersistedReadCache } from "../persistent-read-cache";
import type { ChannelNode, DiscordGuildChannel } from "../types";

// ── localStorage mock (same pattern as persistent-read-cache.test.ts) ────────
const storage = new Map<string, string>();
const mockLocalStorage = {
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => { storage.set(key, value); },
  removeItem: (key: string) => { storage.delete(key); },
  clear: () => { storage.clear(); },
  get length() { return storage.size; },
  key: (_i: number) => null,
};
// @ts-expect-error test mock
globalThis.window = { localStorage: mockLocalStorage };

function buildChannelNode(path: string): ChannelNode {
  return {
    path,
    channelType: null,
    mode: null,
    allowlist: [],
    model: null,
    hasModelField: false,
    displayName: null,
    nameStatus: null,
  };
}

function buildDiscordChannel(
  guildId: string,
  guildName: string,
  channelId: string,
  channelName: string,
): DiscordGuildChannel {
  return {
    guildId,
    guildName,
    channelId,
    channelName,
    defaultAgentId: undefined,
  };
}

// ── loadInitialSharedChannels — tab-switch immediate cache ─────────────────

describe("loadInitialSharedChannels", () => {
  beforeEach(() => { storage.clear(); });

  test("returns null when no cache exists for the instance", () => {
    const result = loadInitialSharedChannels({
      instanceId: "ssh:instance-a",
      instanceToken: 1,
      persistenceScope: "ssh:instance-a",
    });
    expect(result.channelNodes).toBeNull();
    expect(result.discordGuildChannels).toBeNull();
    expect(result.discordChannelsResolved).toBe(false);
  });

  test("returns channelNodes from listChannelsMinimal persistent cache", () => {
    const nodes: ChannelNode[] = [buildChannelNode("channels.discord.guilds.g1.channels.c1.systemPrompt")];
    writePersistedReadCache("ssh:instance-a", "listChannelsMinimal", [], nodes);

    const result = loadInitialSharedChannels({
      instanceId: "ssh:instance-a",
      instanceToken: 1,
      persistenceScope: "ssh:instance-a",
    });
    expect(result.channelNodes).toEqual(nodes);
  });

  test("returns discord channels from listDiscordGuildChannels persistent cache", () => {
    const channels: DiscordGuildChannel[] = [
      buildDiscordChannel("g1", "Guild One", "c1", "general"),
    ];
    writePersistedReadCache("ssh:instance-a", "listDiscordGuildChannels", [], channels);

    const result = loadInitialSharedChannels({
      instanceId: "ssh:instance-a",
      instanceToken: 1,
      persistenceScope: "ssh:instance-a",
    });
    expect(result.discordGuildChannels).toEqual(channels);
  });

  test("discordChannelsResolved is true when all cached channels have resolved names", () => {
    const channels: DiscordGuildChannel[] = [
      buildDiscordChannel("g1", "Guild One", "c1", "general"),
      buildDiscordChannel("g1", "Guild One", "c2", "random"),
    ];
    writePersistedReadCache("ssh:instance-a", "listDiscordGuildChannels", [], channels);

    const result = loadInitialSharedChannels({
      instanceId: "ssh:instance-a",
      instanceToken: 1,
      persistenceScope: "ssh:instance-a",
    });
    expect(result.discordChannelsResolved).toBe(true);
  });

  test("discordChannelsResolved is false when some channels still show raw IDs", () => {
    const channels: DiscordGuildChannel[] = [
      buildDiscordChannel("g1", "Guild One", "c1", "general"),
      buildDiscordChannel("g1", "g1", "c2", "c2"), // unresolved: name === id
    ];
    writePersistedReadCache("ssh:instance-a", "listDiscordGuildChannels", [], channels);

    const result = loadInitialSharedChannels({
      instanceId: "ssh:instance-a",
      instanceToken: 1,
      persistenceScope: "ssh:instance-a",
    });
    expect(result.discordChannelsResolved).toBe(false);
  });

  // ── Switching tabs — the core scenario ────────────────────────────────────

  test("switching instances returns the correct instance's cached data immediately", () => {
    // Simulate two VPS instances with different cached channel lists
    const nodesA: ChannelNode[] = [buildChannelNode("channels.discord.guilds.ga.channels.ca.systemPrompt")];
    const nodesB: ChannelNode[] = [buildChannelNode("channels.discord.guilds.gb.channels.cb.systemPrompt")];
    writePersistedReadCache("ssh:instance-a", "listChannelsMinimal", [], nodesA);
    writePersistedReadCache("ssh:instance-b", "listChannelsMinimal", [], nodesB);

    // Switching to instance B should return B's data, not A's
    const resultB = loadInitialSharedChannels({
      instanceId: "ssh:instance-b",
      instanceToken: 1,
      persistenceScope: "ssh:instance-b",
    });
    expect(resultB.channelNodes).toEqual(nodesB);
    expect(resultB.channelNodes).not.toEqual(nodesA);

    // And A's data is still independently accessible
    const resultA = loadInitialSharedChannels({
      instanceId: "ssh:instance-a",
      instanceToken: 1,
      persistenceScope: "ssh:instance-a",
    });
    expect(resultA.channelNodes).toEqual(nodesA);
  });

  test("switching to a new instance with no cache returns null without showing stale data from another instance", () => {
    const nodesA: ChannelNode[] = [buildChannelNode("channels.discord.guilds.ga.channels.ca.systemPrompt")];
    writePersistedReadCache("ssh:instance-a", "listChannelsMinimal", [], nodesA);

    // Instance B has never been visited — must return null, not A's data
    const resultB = loadInitialSharedChannels({
      instanceId: "ssh:instance-b",
      instanceToken: 1,
      persistenceScope: "ssh:instance-b",
    });
    expect(resultB.channelNodes).toBeNull();
    expect(resultB.discordGuildChannels).toBeNull();
  });

  test("merges fast and full discord channel caches on load", () => {
    // fast cache has the channel structure (IDs only)
    const fastChannels: DiscordGuildChannel[] = [
      buildDiscordChannel("g1", "g1", "c1", "c1"),
    ];
    // full cache has resolved names
    const fullChannels: DiscordGuildChannel[] = [
      buildDiscordChannel("g1", "Guild One", "c1", "general"),
    ];
    writePersistedReadCache("ssh:instance-a", "listDiscordGuildChannelsFast", [], fastChannels);
    writePersistedReadCache("ssh:instance-a", "listDiscordGuildChannels", [], fullChannels);

    const result = loadInitialSharedChannels({
      instanceId: "ssh:instance-a",
      instanceToken: 1,
      persistenceScope: "ssh:instance-a",
    });
    // Resolved names from full cache should win over raw IDs from fast cache
    expect(result.discordGuildChannels?.[0]?.channelName).toBe("general");
    expect(result.discordGuildChannels?.[0]?.guildName).toBe("Guild One");
    expect(result.discordChannelsResolved).toBe(true);
  });
});

describe("channel-cache", () => {
  beforeEach(() => { storage.clear(); });

  test("derives discord placeholders from channel nodes", () => {
    const channels = deriveDiscordGuildChannelsFromChannelNodes([
      buildChannelNode("channels.discord.guilds.guild-1.channels.channel-1.systemPrompt"),
      buildChannelNode("channels.discord.guilds.guild-1.channels.channel-1.model"),
      buildChannelNode("channels.discord.guilds.guild-2.channels.channel-2.systemPrompt"),
      buildChannelNode("channels.telegram.groups.ops.model"),
    ]);

    expect(channels).toEqual([
      buildDiscordChannel("guild-1", "guild-1", "channel-1", "channel-1"),
      buildDiscordChannel("guild-2", "guild-2", "channel-2", "channel-2"),
    ]);
  });

  test("uses cached names while keeping snapshot-discovered coverage", () => {
    const channels = pickInitialSharedDiscordGuildChannels({
      runtimeChannels: [
        buildChannelNode("channels.discord.guilds.guild-1.channels.channel-1.systemPrompt"),
        buildChannelNode("channels.discord.guilds.guild-1.channels.channel-2.systemPrompt"),
      ],
      configChannels: null,
      cachedChannels: [
        buildDiscordChannel("guild-1", "Lay2 Dev", "channel-1", "general"),
      ],
      fastChannels: [
        buildDiscordChannel("guild-1", "guild-1", "channel-2", "channel-2"),
      ],
    });

    expect(channels).toEqual([
      buildDiscordChannel("guild-1", "Lay2 Dev", "channel-1", "general"),
      buildDiscordChannel("guild-1", "guild-1", "channel-2", "channel-2"),
    ]);
  });

  test("merges new placeholder entries without discarding resolved names", () => {
    const merged = mergeDiscordGuildChannels(
      [
        buildDiscordChannel("guild-1", "Lay2 Dev", "channel-1", "general"),
      ],
      [
        buildDiscordChannel("guild-1", "guild-1", "channel-1", "channel-1"),
        buildDiscordChannel("guild-1", "guild-1", "channel-2", "channel-2"),
      ],
    );

    expect(merged).toEqual([
      buildDiscordChannel("guild-1", "Lay2 Dev", "channel-1", "general"),
      buildDiscordChannel("guild-1", "guild-1", "channel-2", "channel-2"),
    ]);
  });

  test("warms shared channels for channel-dependent routes including cron", () => {
    expect(shouldWarmSharedChannels("channels")).toBe(true);
    expect(shouldWarmSharedChannels("cook")).toBe(true);
    expect(shouldWarmSharedChannels("recipes")).toBe(true);
    expect(shouldWarmSharedChannels("recipe-studio")).toBe(true);
    expect(shouldWarmSharedChannels("cron")).toBe(true);
    expect(shouldWarmSharedChannels("home")).toBe(false);
  });
});
