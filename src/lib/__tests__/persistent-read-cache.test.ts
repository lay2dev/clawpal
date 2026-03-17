import { beforeEach, describe, expect, test } from "bun:test";

import {
  buildPersistentReadCacheKey,
  readPersistedReadCache,
  shouldPersistReadMethod,
  writePersistedReadCache,
} from "../persistent-read-cache";

const storage = new Map<string, string>();
const mockLocalStorage = {
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => {
    storage.set(key, value);
  },
  removeItem: (key: string) => {
    storage.delete(key);
  },
  clear: () => {
    storage.clear();
  },
  get length() {
    return storage.size;
  },
  key: (_index: number) => null,
};

// @ts-expect-error test mock
globalThis.window = { localStorage: mockLocalStorage };

describe("persistent read cache", () => {
  beforeEach(() => {
    storage.clear();
  });

  test("persists supported no-arg methods", () => {
    expect(shouldPersistReadMethod("getInstanceConfigSnapshot")).toBe(true);
    expect(shouldPersistReadMethod("listDiscordGuildChannels")).toBe(true);
    expect(shouldPersistReadMethod("listChannelsMinimal")).toBe(true);
    expect(shouldPersistReadMethod("getCronRuns", ["job-1"])).toBe(false);
    expect(shouldPersistReadMethod("listAgents")).toBe(false);
  });

  test("writes and reads values by stable instance scope", () => {
    writePersistedReadCache("ssh:lay2-dev", "getStatusExtra", [], {
      openclawVersion: "2026.3.2",
      duplicateInstalls: [],
    });

    expect(
      readPersistedReadCache("ssh:lay2-dev", "getStatusExtra", []),
    ).toEqual({
      openclawVersion: "2026.3.2",
      duplicateInstalls: [],
    });
  });

  test("returns undefined on malformed stored payload", () => {
    storage.set(
      buildPersistentReadCacheKey("ssh:lay2-dev", "getInstanceRuntimeSnapshot", []),
      "{not-json",
    );

    expect(
      readPersistedReadCache("ssh:lay2-dev", "getInstanceRuntimeSnapshot", []),
    ).toBeUndefined();
  });

  test("does not store unsupported methods", () => {
    writePersistedReadCache("ssh:lay2-dev", "listAgents", [], [{ id: "main" }]);

    expect(storage.size).toBe(0);
    expect(readPersistedReadCache("ssh:lay2-dev", "listAgents", [])).toBeUndefined();
  });

  test("persists listModelProfiles", () => {
    expect(shouldPersistReadMethod("listModelProfiles")).toBe(true);

    const profiles = [
      { id: "p1", provider: "anthropic", model: "claude-sonnet-4-20250514", enabled: true },
      { id: "p2", provider: "openai", model: "gpt-4o", enabled: false },
    ];
    writePersistedReadCache("local", "listModelProfiles", [], profiles);
    expect(readPersistedReadCache("local", "listModelProfiles", [])).toEqual(profiles);
  });
});
