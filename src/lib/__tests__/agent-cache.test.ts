import { describe, expect, test } from "bun:test";

import {
  pickInitialSharedAgents,
  shouldWarmSharedAgents,
} from "../agent-cache";
import type { AgentOverview } from "../types";

function buildAgent(id: string): AgentOverview {
  return {
    id,
    name: id,
    emoji: undefined,
    workspace: undefined,
    model: null,
    channels: [],
    online: true,
  };
}

describe("agent-cache", () => {
  test("prefers runtime snapshot agents over config snapshot and list cache", () => {
    const agents = pickInitialSharedAgents({
      runtimeAgents: [buildAgent("main")],
      configAgents: [buildAgent("backup")],
      cachedAgents: [buildAgent("cached")],
    });

    expect(agents).not.toBeNull();
    expect(agents!.map((agent) => agent.id)).toEqual(["main"]);
  });

  test("falls back to config snapshot, then list cache", () => {
    const configAgents = pickInitialSharedAgents({
        runtimeAgents: null,
        configAgents: [buildAgent("config")],
        cachedAgents: [buildAgent("cached")],
      });
    expect(configAgents).not.toBeNull();
    expect(configAgents!.map((agent) => agent.id)).toEqual(["config"]);

    const cachedAgents = pickInitialSharedAgents({
        runtimeAgents: null,
        configAgents: null,
        cachedAgents: [buildAgent("cached")],
      });
    expect(cachedAgents).not.toBeNull();
    expect(cachedAgents!.map((agent) => agent.id)).toEqual(["cached"]);
  });

  test("warms shared agents for agent-dependent routes and open chat", () => {
    expect(shouldWarmSharedAgents("home", false)).toBe(true);
    expect(shouldWarmSharedAgents("channels", false)).toBe(true);
    expect(shouldWarmSharedAgents("cook", false)).toBe(true);
    expect(shouldWarmSharedAgents("recipes", false)).toBe(false);
    expect(shouldWarmSharedAgents("doctor", true)).toBe(true);
  });
});
