import { describe, expect, test } from "bun:test";

import {
  pickInitialSharedModelProfiles,
  shouldWarmSharedModelProfiles,
} from "../model-profile-cache";
import type { ModelProfile } from "../types";

function buildProfile(
  id: string,
  enabled: boolean,
): ModelProfile {
  return {
    id,
    name: id,
    provider: "anthropic",
    model: "claude-sonnet-4-20250514",
    authRef: "providers.anthropic",
    enabled,
  };
}

describe("model-profile-cache", () => {
  test("prefers cached instance profiles and filters disabled entries", () => {
    const profiles = pickInitialSharedModelProfiles({
      cachedProfiles: [
        buildProfile("cached-enabled", true),
        buildProfile("cached-disabled", false),
      ],
      persistedProfiles: [
        buildProfile("persisted-enabled", true),
      ],
    });

    expect(profiles).toEqual([
      buildProfile("cached-enabled", true),
    ]);
  });

  test("falls back to persisted profiles when cache is empty", () => {
    const profiles = pickInitialSharedModelProfiles({
      cachedProfiles: null,
      persistedProfiles: [
        buildProfile("persisted-enabled", true),
        buildProfile("persisted-disabled", false),
      ],
    });

    expect(profiles).toEqual([
      buildProfile("persisted-enabled", true),
    ]);
  });

  test("warms shared model profiles for pages that render model selectors", () => {
    expect(shouldWarmSharedModelProfiles("home")).toBe(true);
    expect(shouldWarmSharedModelProfiles("channels")).toBe(true);
    expect(shouldWarmSharedModelProfiles("cook")).toBe(true);
    expect(shouldWarmSharedModelProfiles("recipes")).toBe(false);
    expect(shouldWarmSharedModelProfiles("cron")).toBe(false);
  });
});
