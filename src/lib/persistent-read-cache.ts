const STORAGE_PREFIX = "clawpal_persisted_read_cache_v1";

const PERSISTED_READ_METHODS = new Set([
  "getInstanceConfigSnapshot",
  "getInstanceRuntimeSnapshot",
  "getStatusExtra",
  "getChannelsConfigSnapshot",
  "getChannelsRuntimeSnapshot",
  "listChannelsMinimal",
  "listDiscordGuildChannels",
  "getCronConfigSnapshot",
  "getCronRuntimeSnapshot",
  "getRescueBotStatus",
  "listModelProfiles",
]);

type PersistedReadCacheEntry<T> = {
  persistedAt: number;
  value: T;
};

function getStorage(): Storage | null {
  if (typeof window === "undefined" || !window.localStorage) {
    return null;
  }
  return window.localStorage;
}

function serializeArgs(args: unknown[]): string {
  try {
    return JSON.stringify(args);
  } catch {
    return String(args.length);
  }
}

export function shouldPersistReadMethod(method: string, args: unknown[] = []): boolean {
  return args.length === 0 && PERSISTED_READ_METHODS.has(method);
}

export function buildPersistentReadCacheKey(
  instanceScope: string,
  method: string,
  args: unknown[] = [],
): string {
  return `${STORAGE_PREFIX}:${instanceScope}:${method}:${serializeArgs(args)}`;
}

export function readPersistedReadCache<T>(
  instanceScope: string,
  method: string,
  args: unknown[] = [],
): T | undefined {
  if (!shouldPersistReadMethod(method, args)) {
    return undefined;
  }
  const storage = getStorage();
  if (!storage) {
    return undefined;
  }
  const raw = storage.getItem(buildPersistentReadCacheKey(instanceScope, method, args));
  if (!raw) {
    return undefined;
  }
  try {
    const parsed = JSON.parse(raw) as PersistedReadCacheEntry<T>;
    if (!parsed || typeof parsed !== "object" || !("value" in parsed)) {
      return undefined;
    }
    return parsed.value;
  } catch {
    return undefined;
  }
}

export function writePersistedReadCache<T>(
  instanceScope: string,
  method: string,
  args: unknown[] = [],
  value: T,
): void {
  if (!shouldPersistReadMethod(method, args)) {
    return;
  }
  const storage = getStorage();
  if (!storage) {
    return;
  }
  const payload: PersistedReadCacheEntry<T> = {
    persistedAt: Date.now(),
    value,
  };
  try {
    storage.setItem(
      buildPersistentReadCacheKey(instanceScope, method, args),
      JSON.stringify(payload),
    );
  } catch {
    // Best-effort persistence only.
  }
}
