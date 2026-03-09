import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";

import i18n from "@/i18n";
import { InstallHubEntryCards } from "../InstallHub";

describe("InstallHub", () => {
  test("stacks connect entry cards vertically and keeps text wrap-friendly", async () => {
    await i18n.changeLanguage("en");

    const html = renderToStaticMarkup(
      React.createElement(I18nextProvider, {
        i18n,
        children: React.createElement(InstallHubEntryCards, {
          onConnectRemote: () => {},
          onConnectDocker: () => {},
          onConnectWsl2: () => {},
        }),
      }),
    );

    expect(html).toContain("grid gap-3");
    expect(html).not.toContain("sm:grid-cols-2");
    expect(html).not.toContain("xl:grid-cols-3");
    expect(html).toContain("w-full");
    expect(html).toContain("whitespace-normal");
    expect(html).toContain("break-words");
  });

  test("ships dedicated install hub failure translations", async () => {
    await i18n.changeLanguage("en");
    expect(i18n.t("installChat.connectionFailed")).toBe("Connection failed");
    expect(i18n.t("installChat.back")).toBe("Back");

    await i18n.changeLanguage("zh");
    expect(i18n.t("installChat.connectionFailed")).toBe("连接失败");
    expect(i18n.t("installChat.back")).toBe("返回");
  });
});
