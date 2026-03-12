import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { RecipePlanPreview } from "../RecipePlanPreview";

describe("RecipePlanPreview", () => {
  test("renders capability and resource summaries in the confirm phase", () => {
    const html = renderToStaticMarkup(
      React.createElement(RecipePlanPreview, {
        routeSummary: "remote_ssh -> ssh:prod-a",
        authIssues: [
          {
            code: "AUTH_CREDENTIAL_UNRESOLVED",
            severity: "error",
            message: "missing auth",
            autoFixable: false,
          },
        ],
        contextWarnings: ["Channel discord/channel-1 will be rebound from main to lobster."],
        plan: {
          summary: {
            recipeId: "discord-channel-persona",
            recipeName: "Channel Persona",
            executionKind: "attachment",
            actionCount: 1,
            skippedStepCount: 0,
          },
          usedCapabilities: ["service.manage"],
          concreteClaims: [{ kind: "path", path: "~/.openclaw/config.json" }],
          executionSpecDigest: "digest-123",
          executionSpec: {
            apiVersion: "strategy.platform/v1",
            kind: "ExecutionSpec",
            metadata: {},
            source: {},
            target: {},
            execution: {
              kind: "attachment",
            },
            capabilities: {
              usedCapabilities: ["service.manage"],
            },
            resources: {
              claims: [{ kind: "path", path: "~/.openclaw/config.json" }],
            },
            secrets: {
              bindings: [],
            },
            desiredState: {},
            actions: [],
            outputs: [],
          },
          warnings: [],
        },
      }),
    );

    expect(html).toContain("service.manage");
    expect(html).toContain("path");
    expect(html).toContain("digest-123");
    expect(html).toContain("remote_ssh -&gt; ssh:prod-a");
    expect(html).toContain("AUTH_CREDENTIAL_UNRESOLVED");
    expect(html).toContain("will be rebound");
  });
});
