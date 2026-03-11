import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";

import i18n from "@/i18n";
import { History } from "../History";

describe("History runtime association", () => {
  test("shows runtime details for snapshots linked to recipe runs", async () => {
    await i18n.changeLanguage("en");

    const html = renderToStaticMarkup(
      React.createElement(I18nextProvider, {
        i18n,
        children: React.createElement(History, {
          initialHistory: [
            {
              id: "snapshot_01",
              recipeId: "discord-channel-persona",
              createdAt: "2026-03-11T10:00:03Z",
              source: "clawpal",
              canRollback: true,
              runId: "run_01",
            },
          ],
          initialRuns: [
            {
              id: "run_01",
              instanceId: "local",
              recipeId: "discord-channel-persona",
              executionKind: "attachment",
              runner: "local",
              status: "succeeded",
              summary: "Applied persona patch",
              startedAt: "2026-03-11T10:00:00Z",
              finishedAt: "2026-03-11T10:00:03Z",
              artifacts: [
                {
                  id: "artifact_01",
                  kind: "configDiff",
                  label: "Rendered patch",
                },
              ],
              resourceClaims: [
                {
                  kind: "path",
                  id: "openclaw.config",
                  path: "~/.openclaw/openclaw.json",
                },
              ],
              warnings: [],
            },
          ],
        }),
      }),
    );

    expect(html).toContain("Applied persona patch");
    expect(html).toContain("Run ID");
    expect(html).toContain("run_01");
    expect(html).toContain("Rendered patch");
    expect(html).toContain("openclaw.config");
  });
});
