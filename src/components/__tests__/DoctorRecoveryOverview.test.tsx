import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";

import i18n from "@/i18n";
import type { RescuePrimaryDiagnosisResult } from "@/lib/types";
import { DoctorRecoveryOverview } from "../DoctorRecoveryOverview";

describe("DoctorRecoveryOverview", () => {
  test("renders a single global fix surface above sections", async () => {
    await i18n.changeLanguage("en");
    const diagnosis: RescuePrimaryDiagnosisResult = {
      status: "broken",
      checkedAt: "2026-03-07T00:00:00Z",
      targetProfile: "primary",
      rescueProfile: "rescue",
      rescueConfigured: true,
      rescuePort: 19789,
      summary: {
        status: "broken",
        headline: "Gateway needs attention first",
        recommendedAction: "Apply 1 safe fix and re-run recovery",
        fixableIssueCount: 1,
        selectedFixIssueIds: ["field.agents"],
      },
      sections: [
        {
          key: "agents",
          title: "Agents",
          status: "degraded",
          summary: "Agents has 1 recommended change",
          docsUrl: "https://docs.openclaw.ai/agents",
          items: [
            {
              id: "field.agents",
              label: "Missing agent defaults",
              status: "warn",
              detail: "Initialize agents.defaults.model",
              autoFixable: true,
              issueId: "field.agents",
            },
          ],
        },
        {
          key: "gateway",
          title: "Gateway",
          status: "healthy",
          summary: "Gateway checks look healthy",
          docsUrl: "https://docs.openclaw.ai/gateway/security/index",
          items: [
            {
              id: "field.gateway.port",
              label: "Gateway port",
              status: "ok",
              detail: "Configured primary gateway port: 18789",
              autoFixable: false,
              issueId: null,
            },
          ],
        },
      ],
      checks: [],
      issues: [],
    };

    const html = renderToStaticMarkup(
      React.createElement(I18nextProvider, {
        i18n,
        children: React.createElement(DoctorRecoveryOverview, {
          diagnosis,
          checkLoading: false,
          repairing: false,
          progressLine: null,
          repairResult: null,
          repairError: null,
          onRepairAll: () => {},
          onRepairIssue: () => {},
        }),
      }),
    );

    expect(html).toContain("Gateway needs attention first");
    expect(html).toContain("Apply 1 safe fix and re-run recovery");
    expect(html).toContain("Fix 1 safe issue");
    expect(html).toContain("Agents");
    expect(html).toContain("Gateway");
    expect(html.match(/Fix 1 safe issue/g)?.length ?? 0).toBe(1);
    expect(html).toContain("Open Gateway docs");
    expect(html).toContain("text-emerald-700");
    expect(html).toContain("<details open");
    expect(html).toContain("<details");
  });
});
