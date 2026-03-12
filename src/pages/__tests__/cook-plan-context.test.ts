import { describe, expect, test } from "bun:test";

import type { RecipePlan } from "@/lib/types";
import {
  buildCookContextWarnings,
  buildCookRouteSummary,
  hasBlockingAuthIssues,
} from "../cook-plan-context";

const samplePlan: RecipePlan = {
  summary: {
    recipeId: "dedicated-channel-agent",
    recipeName: "Create dedicated Agent for Channel",
    executionKind: "job",
    actionCount: 4,
    skippedStepCount: 0,
  },
  usedCapabilities: ["agent.manage", "binding.manage", "config.write"],
  concreteClaims: [],
  executionSpecDigest: "digest-123",
  executionSpec: {
    apiVersion: "strategy.platform/v1",
    kind: "ExecutionSpec",
    metadata: { name: "dedicated-channel-agent" },
    source: {},
    target: {},
    execution: { kind: "job" as const },
    capabilities: { usedCapabilities: ["agent.manage"] },
    resources: { claims: [] },
    secrets: { bindings: [] },
    desiredState: {},
    actions: [
      {
        kind: "bind_channel",
        args: {
          channelType: "discord",
          peerId: "channel-1",
          agentId: "lobster",
        },
      },
      {
        kind: "config_patch",
        args: {
          patch: {
            channels: {
              discord: {
                guilds: {
                  "guild-1": {
                    channels: {
                      "channel-1": {
                        systemPrompt: "new persona",
                      },
                    },
                  },
                },
              },
            },
          },
        },
      },
    ],
    outputs: [],
  },
  warnings: [],
};

describe("cook plan context helpers", () => {
  test("describes remote ssh execution routing", () => {
    expect(
      buildCookRouteSummary({
        instanceId: "ssh:prod-a",
        instanceLabel: "Prod A",
        isRemote: true,
        isDocker: false,
      }),
    ).toEqual({
      kind: "ssh",
      targetLabel: "Prod A",
    });
  });

  test("warns when a channel binding will be reassigned", () => {
    const warnings = buildCookContextWarnings(
      samplePlan,
      JSON.stringify({
        bindings: [
          {
            agentId: "main",
            match: {
              channel: "discord",
              peer: { kind: "channel", id: "channel-1" },
            },
          },
        ],
      }),
    );

    expect(warnings.some((warning) => warning.includes("will be rebound"))).toBe(true);
  });

  test("warns when channel config will overwrite an existing value", () => {
    const warnings = buildCookContextWarnings(
      samplePlan,
      JSON.stringify({
        channels: {
          discord: {
            guilds: {
              "guild-1": {
                channels: {
                  "channel-1": {
                    systemPrompt: "old persona",
                  },
                },
              },
            },
          },
        },
      }),
    );

    expect(warnings.some((warning) => warning.includes("systemPrompt"))).toBe(true);
  });

  test("treats auth errors as blocking", () => {
    expect(
      hasBlockingAuthIssues([
        {
          code: "AUTH_CREDENTIAL_UNRESOLVED",
          severity: "error",
          message: "missing auth",
          autoFixable: false,
        },
      ]),
    ).toBe(true);
  });
});
