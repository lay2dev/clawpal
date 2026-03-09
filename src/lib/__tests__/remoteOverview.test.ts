import { describe, expect, test } from "bun:test";

import { mergeOverviewSnapshot } from "../remoteOverview";

describe("mergeOverviewSnapshot", () => {
  test("keeps config snapshot when runtime snapshot is missing", () => {
    expect(
      mergeOverviewSnapshot(
        {
          defaultModel: "openai/gpt-5",
          fallbackModels: ["anthropic/claude-3.7"],
        },
        null,
      ),
    ).toEqual({
      defaultModel: "openai/gpt-5",
      fallbackModels: ["anthropic/claude-3.7"],
    });
  });

  test("runtime snapshot overwrites scalar, array, and object fields from config", () => {
    expect(
      mergeOverviewSnapshot(
        {
          defaultModel: "openai/gpt-5",
          fallbackModels: ["anthropic/claude-3.7"],
          bindings: [{ agentId: "main", match: { channel: "discord" } }],
          watchdog: { alive: false, deployed: true },
        },
        {
          defaultModel: "openai/gpt-5.3-codex",
          fallbackModels: ["openai/gpt-5-mini"],
          bindings: [{ agentId: "agent-2", match: { channel: "discord" } }],
          watchdog: { alive: true, deployed: true },
        },
      ),
    ).toEqual({
      defaultModel: "openai/gpt-5.3-codex",
      fallbackModels: ["openai/gpt-5-mini"],
      bindings: [{ agentId: "agent-2", match: { channel: "discord" } }],
      watchdog: { alive: true, deployed: true },
    });
  });

  test("runtime snapshot can add fields that are absent from config", () => {
    expect(
      mergeOverviewSnapshot<
        { channels: { path: string }[] },
        { agents: { id: string; online: boolean }[] }
      >(
        {
          channels: [{ path: "channels.discord" }],
        },
        {
          agents: [{ id: "main", online: true }],
        },
      ),
    ).toEqual({
      channels: [{ path: "channels.discord" }],
      agents: [{ id: "main", online: true }],
    });
  });
});
