import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";

import i18n from "@/i18n";
import { InstanceContext } from "@/lib/instance-context";
import { Doctor } from "../Doctor";

describe("Doctor page rescue header", () => {
  test("centers the LED bot, keeps icon controls underneath, and uses icon-only primary checks", async () => {
    await i18n.changeLanguage("en");

    const html = renderToStaticMarkup(
      React.createElement(I18nextProvider, {
        i18n,
        children: React.createElement(InstanceContext.Provider, {
          value: {
            instanceId: "local",
            instanceToken: 0,
            isRemote: false,
            isDocker: false,
            isConnected: true,
            channelNodes: null,
            discordGuildChannels: null,
            channelsLoading: false,
            discordChannelsLoading: false,
            refreshChannelNodesCache: async () => [],
            refreshDiscordChannelsCache: async () => [],
          },
          children: React.createElement(Doctor, {}),
        }),
      }),
    );

    expect(html).toContain("flex flex-col items-center");
    expect(html).toContain("data-led-bot=\"wide-console\"");
    expect(html).toContain("More options");
    expect(html).toContain("aria-label=\"Open logs\"");
    expect(html).toContain("aria-label=\"Play\"");
    expect(html).not.toContain(">Enable<");
    expect(html).not.toContain("Port");
    expect(html).not.toContain("Enable the helper when you want a safe sidecar for diagnosis and repair.");
    expect(html).not.toContain("Rescue Bot");
    expect(html).not.toContain("Activate Rescue Bot");
    expect(html).not.toContain("More Rescue Bot actions");
    expect(html.indexOf("More options")).toBeLessThan(
      html.indexOf("Safe checks and guided fixes before touching your main gateway."),
    );
    expect(i18n.t("doctor.primaryRecoveryTitle")).toBe("Check Primary Agent");
    expect(i18n.t("doctor.primaryCheckNow")).toBe("Check Primary Agent");
    expect(html).not.toContain(">Check Primary Agent<");
  });
});
