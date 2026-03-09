import type { SshHost } from "./types";

const REMOTE_PERSISTENCE_SCOPE_KEY = "clawpal_remote_persistence_scope_v1";

type RemotePersistenceScopeEntry = {
  fingerprint: string;
  scope: string;
  verifiedAt: number;
};

type RemotePersistenceScopeIndex = Record<string, RemotePersistenceScopeEntry>;

function getStorage(): Storage | null {
  if (typeof window === "undefined" || !window.localStorage) {
    return null;
  }
  return window.localStorage;
}

function normalizeOptionalString(value: string | undefined): string {
  return value?.trim() || "";
}

function readRemotePersistenceScopeIndex(): RemotePersistenceScopeIndex {
  const storage = getStorage();
  if (!storage) return {};
  const raw = storage.getItem(REMOTE_PERSISTENCE_SCOPE_KEY);
  if (!raw) return {};
  try {
    const parsed = JSON.parse(raw) as RemotePersistenceScopeIndex;
    if (!parsed || typeof parsed !== "object") {
      return {};
    }
    return parsed;
  } catch {
    return {};
  }
}

function writeRemotePersistenceScopeIndex(index: RemotePersistenceScopeIndex): void {
  const storage = getStorage();
  if (!storage) return;
  try {
    storage.setItem(REMOTE_PERSISTENCE_SCOPE_KEY, JSON.stringify(index));
  } catch {
    // Best-effort persistence only.
  }
}

function buildRemoteScopeId(hostId: string): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return `ssh-scope:${hostId}:${crypto.randomUUID()}`;
  }
  return `ssh-scope:${hostId}:${Date.now()}`;
}

export function buildRemotePersistenceFingerprint(host: Pick<SshHost, "host" | "port" | "username" | "authMethod" | "keyPath">): string {
  return JSON.stringify({
    host: host.host.trim().toLowerCase(),
    port: host.port,
    username: host.username.trim().toLowerCase(),
    authMethod: host.authMethod,
    keyPath: normalizeOptionalString(host.keyPath),
  });
}

export function readRemotePersistenceScope(host: Pick<SshHost, "id" | "host" | "port" | "username" | "authMethod" | "keyPath">): string | null {
  const entry = readRemotePersistenceScopeIndex()[host.id];
  if (!entry) return null;
  if (entry.fingerprint !== buildRemotePersistenceFingerprint(host)) {
    return null;
  }
  return entry.scope;
}

export function ensureRemotePersistenceScope(host: Pick<SshHost, "id" | "host" | "port" | "username" | "authMethod" | "keyPath">): string {
  const existing = readRemotePersistenceScope(host);
  if (existing) return existing;

  const nextScope = buildRemoteScopeId(host.id);
  const index = readRemotePersistenceScopeIndex();
  index[host.id] = {
    fingerprint: buildRemotePersistenceFingerprint(host),
    scope: nextScope,
    verifiedAt: Date.now(),
  };
  writeRemotePersistenceScopeIndex(index);
  return nextScope;
}

export function clearRemotePersistenceScope(hostId: string): void {
  const index = readRemotePersistenceScopeIndex();
  if (!(hostId in index)) return;
  delete index[hostId];
  writeRemotePersistenceScopeIndex(index);
}
