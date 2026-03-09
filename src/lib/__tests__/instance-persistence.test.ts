import { beforeEach, describe, expect, test } from "bun:test";

import {
  buildRemotePersistenceFingerprint,
  clearRemotePersistenceScope,
  ensureRemotePersistenceScope,
  readRemotePersistenceScope,
} from "../instance-persistence";

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

const host = {
  id: "ssh:prod-box",
  host: "prod.example.com",
  port: 22,
  username: "root",
  authMethod: "key" as const,
  keyPath: "~/.ssh/id_ed25519",
};

describe("instance persistence", () => {
  beforeEach(() => {
    storage.clear();
  });

  test("returns no persistence scope for a host that has never connected", () => {
    expect(readRemotePersistenceScope(host)).toBeNull();
  });

  test("reuses the same persistence scope for the same verified remote host", () => {
    const first = ensureRemotePersistenceScope(host);
    const second = ensureRemotePersistenceScope(host);

    expect(second).toBe(first);
    expect(readRemotePersistenceScope(host)).toBe(first);
  });

  test("invalidates a remembered scope when the host identity changes", () => {
    const scope = ensureRemotePersistenceScope(host);

    expect(scope).toContain("ssh-scope:ssh:prod-box:");
    expect(readRemotePersistenceScope({
      ...host,
      port: 2222,
    })).toBeNull();
  });

  test("clears a remembered scope when the SSH host is deleted or recreated", () => {
    const scope = ensureRemotePersistenceScope(host);

    clearRemotePersistenceScope(host.id);

    expect(scope).toContain("ssh-scope:ssh:prod-box:");
    expect(readRemotePersistenceScope(host)).toBeNull();
  });

  test("builds a stable host fingerprint from connection-defining fields", () => {
    expect(buildRemotePersistenceFingerprint(host)).toBe(
      buildRemotePersistenceFingerprint({
        ...host,
        host: "PROD.EXAMPLE.COM",
        username: "ROOT",
      }),
    );
  });
});
