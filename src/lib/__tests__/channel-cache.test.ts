import { describe, expect, test } from "bun:test";

import {
  deriveDiscordGuildChannelsFromChannelNodes,
  mergeDiscordGuildChannels,
  pickInitialSharedDiscordGuildChannels,
  shouldWarmSharedChannels,
} from "../channel-cache";
import type { ChannelNode, DiscordGuildChannel } from "../types";

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

describe("channel-cache", () => {
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
