import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";

import i18n from "@/i18n";
import { InstanceContext } from "@/lib/instance-context";
import { Channels } from "../Channels";

describe("Channels page cached discord rendering", () => {
  test("keeps cached discord channels visible while a background refresh is running", async () => {
    await i18n.changeLanguage("en");

    const html = renderToStaticMarkup(
      React.createElement(I18nextProvider, {
        i18n,
        children: React.createElement(InstanceContext.Provider, {
          value: {
            instanceId: "ssh:hetzner",
            instanceViewToken: "ssh:hetzner",
            instanceToken: 1,
            persistenceScope: "ssh-scope:ssh:hetzner:test",
            persistenceResolved: true,
            isRemote: true,
            isDocker: false,
            isConnected: true,
            channelNodes: null,
            discordGuildChannels: [
              {
                guildId: "guild-1",
                guildName: "Lay2 Dev",
                channelId: "channel-1",
                channelName: "general",
                defaultAgentId: undefined,
              },
            ],
            channelsLoading: false,
            discordChannelsLoading: true,
            discordChannelsResolved: false,
            agents: [],
            agentsLoading: false,
            setAgentsCache: () => null,
            refreshAgentsCache: async () => [],
            refreshChannelNodesCache: async () => [],
            refreshDiscordChannelsCache: async () => [],
          },
          children: React.createElement(Channels, {}),
        }),
      }),
    );

    expect(html).toContain("Lay2 Dev");
    expect(html).toContain("general");
    expect(html).not.toContain("Loading Discord channels");
  });
});
