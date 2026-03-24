import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";

import i18n from "@/i18n";
import { OpenclawContext } from "../OpenclawContext";

describe("OpenclawContext", () => {
  test("renders a localized title with sessions and backups sections", async () => {
    await i18n.changeLanguage("zh");

    const html = renderToStaticMarkup(
      React.createElement(I18nextProvider, {
        i18n,
        children: React.createElement(OpenclawContext),
      }),
    );

    expect(html).toContain(">上下文<");
    expect(html).toContain(">会话<");
    expect(html).toContain(">备份<");
  });
});
