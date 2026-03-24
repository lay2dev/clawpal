import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";

import i18n from "@/i18n";
import { SettingsAlphaFeaturesCard } from "../SettingsAlphaFeaturesCard";

describe("SettingsAlphaFeaturesCard", () => {
  test("only shows the ssh transfer speed toggle", async () => {
    await i18n.changeLanguage("en");

    const html = renderToStaticMarkup(
      React.createElement(I18nextProvider, {
        i18n,
        children: React.createElement(SettingsAlphaFeaturesCard, {
          showSshTransferSpeedUi: false,
          remoteDoctorInviteCode: "",
          onSshTransferSpeedUiToggle: () => {},
          onRemoteDoctorInviteCodeChange: () => {},
          onRemoteDoctorInviteCodeSave: () => {},
        }),
      }),
    );

    expect(html).toContain("SSH transfer speed");
    expect(html).toContain("Remote Doctor Invite Code");
    expect(html).not.toContain("Remote Doctor Gateway URL");
    expect(html).not.toContain("Remote Doctor Gateway Auth Token");
    expect(html).not.toContain("ClawPal Logs");
    expect(html).not.toContain("OpenClaw Gateway Logs");
    expect(html).not.toContain("OpenClaw Context");
    expect(html).not.toContain("Enable Doctor Claw (Alpha)");
    expect(html).not.toContain("Enable Rescue Bot (Alpha)");
  });
});
