/**
 * Read-through cache layer for Tauri IPC and remote API calls.
 * Extracted from use-api.ts for readability.
 */
import { invoke } from "@tauri-apps/api/core";
import { api } from "./api";
import { extractErrorText } from "./sshDiagnostic";
import {
  createDataLoadRequestId,
  emitDataLoadMetric,
  inferDataLoadPage,
  inferDataLoadSource,
  parseInstanceToken,
} from "./data-load-log";
import { writePersistedReadCache } from "./persistent-read-cache";

export function hasGuidanceEmitted(error: unknown): boolean {
  return !!(error && typeof error === "object" && (error as any)._guidanceEmitted);
}

type ApiReadCacheEntry = {
  expiresAt: number;
  value: unknown;
  inFlight?: Promise<unknown>;
  /** If > Date.now(), this entry is "pinned" by an optimistic update and polls should not overwrite it. */
  optimisticUntil?: number;
};

const API_READ_CACHE = new Map<string, ApiReadCacheEntry>();
const API_READ_CACHE_MAX_ENTRIES = 512;

/** Subscribers keyed by cache key; notified on cache value changes. */
const _cacheSubscribers = new Map<string, Set<() => void>>();

function _notifyCacheSubscribers(key: string) {
  const subs = _cacheSubscribers.get(key);
  if (subs) {
    for (const fn of subs) fn();
  }
}

/** Subscribe to changes on a specific cache key. Returns an unsubscribe function. */
export function subscribeToCacheKey(key: string, callback: () => void): () => void {
  let set = _cacheSubscribers.get(key);
  if (!set) {
    set = new Set();
    _cacheSubscribers.set(key, set);
  }
  set.add(callback);
  return () => {
    set!.delete(callback);
    if (set!.size === 0) _cacheSubscribers.delete(key);
  };
}

/** Read the current cached value for a key (if any). */
export function readCacheValue<T>(key: string): T | undefined {
  const entry = API_READ_CACHE.get(key);
  return entry?.value as T | undefined;
}

export function buildCacheKey(instanceCacheKey: string, method: string, args: unknown[] = []): string {
  return makeCacheKey(instanceCacheKey, method, args);
}

const HOST_SHARED_READ_METHODS = new Set([
  "getInstanceConfigSnapshot",
  "getInstanceRuntimeSnapshot",
  "getStatusExtra",
  "getChannelsConfigSnapshot",
  "getChannelsRuntimeSnapshot",
  "getCronConfigSnapshot",
  "getCronRuntimeSnapshot",
  "getRescueBotStatus",
  "checkOpenclawUpdate",
]);

export function resolveReadCacheScopeKey(
  instanceCacheKey: string,
  persistenceScope: string | null,
  method: string,
): string {
  if (HOST_SHARED_READ_METHODS.has(method) && persistenceScope) {
    return persistenceScope;
  }
  return instanceCacheKey;
}

export function makeCacheKey(instanceCacheKey: string, method: string, args: unknown[]): string {
  let serializedArgs = "";
  try {
    serializedArgs = JSON.stringify(args);
  } catch {
    serializedArgs = String(args.length);
  }
  return `${instanceCacheKey}:${method}:${serializedArgs}`;
}

function trimReadCacheIfNeeded() {
  if (API_READ_CACHE.size <= API_READ_CACHE_MAX_ENTRIES) return;
  const deleteCount = API_READ_CACHE.size - API_READ_CACHE_MAX_ENTRIES;
  const keys = API_READ_CACHE.keys();
  for (let i = 0; i < deleteCount; i += 1) {
    const next = keys.next();
    if (next.done) break;
    API_READ_CACHE.delete(next.value);
  }
}

export function invalidateReadCacheForInstance(instanceCacheKey: string, methods?: string[]) {
  const methodSet = methods ? new Set(methods) : null;
  for (const key of API_READ_CACHE.keys()) {
    if (!key.startsWith(`${instanceCacheKey}:`)) continue;
    if (!methodSet) {
      API_READ_CACHE.delete(key);
      _notifyCacheSubscribers(key);
      continue;
    }
    const method = key.slice(instanceCacheKey.length + 1).split(":", 1)[0];
    if (methodSet.has(method)) {
      API_READ_CACHE.delete(key);
      _notifyCacheSubscribers(key);
    }
  }
}

export function invalidateGlobalReadCache(methods?: string[]) {
  invalidateReadCacheForInstance("__global__", methods);
}

/**
 * Set an optimistic value for a cache key, "pinning" it so that polling
 * results will NOT overwrite it for `pinDurationMs` (default 15s).
 *
 * This solves the race condition where:
 *   mutation → optimistic setState → poll fires → stale cache → UI flickers back
 *
 * The pin auto-expires, so if the backend takes longer than expected,
 * the next poll after expiry will overwrite with fresh data.
 */
export function setOptimisticReadCache<T>(
  key: string,
  value: T,
  pinDurationMs = 15_000,
) {
  const existing = API_READ_CACHE.get(key);
  API_READ_CACHE.set(key, {
    value,
    expiresAt: Date.now() + pinDurationMs, // Keep it "valid" for the pin duration
    optimisticUntil: Date.now() + pinDurationMs,
    inFlight: existing?.inFlight,
  });
  _notifyCacheSubscribers(key);
}

export function primeReadCache<T>(
  key: string,
  value: T,
  ttlMs: number,
) {
  API_READ_CACHE.set(key, {
    value,
    expiresAt: Date.now() + ttlMs,
    optimisticUntil: undefined,
  });
  trimReadCacheIfNeeded();
  _notifyCacheSubscribers(key);
}

export async function prewarmRemoteInstanceReadCache(
  instanceId: string,
  instanceToken: number,
  persistenceScope: string | null,
) {
  const instanceCacheKey = `${instanceId}#${instanceToken}`;
  const warm = <T,>(
    method: string,
    ttlMs: number,
    loader: () => Promise<T>,
  ) => callWithReadCache(
    resolveReadCacheScopeKey(instanceCacheKey, persistenceScope, method),
    instanceId,
    persistenceScope,
    method,
    [],
    ttlMs,
    loader,
  ).catch(() => undefined);

  void warm(
    "getInstanceConfigSnapshot",
    20_000,
    () => api.remoteGetInstanceConfigSnapshot(instanceId),
  );
  void warm(
    "getInstanceRuntimeSnapshot",
    10_000,
    () => api.remoteGetInstanceRuntimeSnapshot(instanceId),
  );
  void warm(
    "getStatusExtra",
    15_000,
    () => api.remoteGetStatusExtra(instanceId),
  );
  void warm(
    "getChannelsConfigSnapshot",
    20_000,
    () => api.remoteGetChannelsConfigSnapshot(instanceId),
  );
  void warm(
    "getChannelsRuntimeSnapshot",
    12_000,
    () => api.remoteGetChannelsRuntimeSnapshot(instanceId),
  );
  void warm(
    "getCronConfigSnapshot",
    20_000,
    () => api.remoteGetCronConfigSnapshot(instanceId),
  );
  void warm(
    "getCronRuntimeSnapshot",
    12_000,
    () => api.remoteGetCronRuntimeSnapshot(instanceId),
  );
  void warm(
    "getRescueBotStatus",
    8_000,
    () => api.remoteGetRescueBotStatus(instanceId),
  );
}

export function callWithReadCache<TResult>(
  instanceCacheKey: string,
  metricInstanceId: string,
  persistenceScope: string | null,
  method: string,
  args: unknown[],
  ttlMs: number,
  loader: () => Promise<TResult>,
): Promise<TResult> {
  if (ttlMs <= 0) return loader();
  const now = Date.now();
  const key = makeCacheKey(instanceCacheKey, method, args);
  const page = inferDataLoadPage(method);
  const instanceToken = parseInstanceToken(instanceCacheKey);
  const entry = API_READ_CACHE.get(key);
  if (entry) {
    // If pinned by optimistic update, return the pinned value
    if (entry.optimisticUntil && entry.optimisticUntil > now) {
      emitDataLoadMetric({
        requestId: createDataLoadRequestId(method),
        resource: method,
        page,
        instanceId: metricInstanceId,
        instanceToken,
        source: "cache",
        phase: "success",
        elapsedMs: 0,
        cacheHit: true,
      });
      return Promise.resolve(entry.value as TResult);
    }
    if (entry.expiresAt > now) {
      emitDataLoadMetric({
        requestId: createDataLoadRequestId(method),
        resource: method,
        page,
        instanceId: metricInstanceId,
        instanceToken,
        source: "cache",
        phase: "success",
        elapsedMs: 0,
        cacheHit: true,
      });
      return Promise.resolve(entry.value as TResult);
    }
    if (entry.inFlight) {
      return entry.inFlight as Promise<TResult>;
    }
  }
  const requestId = createDataLoadRequestId(method);
  const startedAt = Date.now();
  const source = inferDataLoadSource(method);
  emitDataLoadMetric({
    requestId,
    resource: method,
    page,
    instanceId: metricInstanceId,
    instanceToken,
    source,
    phase: "start",
    elapsedMs: 0,
    cacheHit: false,
  });
  const request = loader()
    .then((value) => {
      const elapsedMs = Date.now() - startedAt;
      const current = API_READ_CACHE.get(key);
      // Don't overwrite if a newer optimistic value was set while we were fetching
      if (current?.optimisticUntil && current.optimisticUntil > Date.now()) {
        // Clear inFlight but keep the optimistic value
        API_READ_CACHE.set(key, {
          ...current,
          inFlight: undefined,
        });
        emitDataLoadMetric({
          requestId,
          resource: method,
          page,
          instanceId: metricInstanceId,
          instanceToken,
          source,
          phase: "success",
          elapsedMs,
          cacheHit: false,
        });
        return current.value as TResult;
      }
      API_READ_CACHE.set(key, {
        value,
        expiresAt: Date.now() + ttlMs,
        optimisticUntil: undefined,
      });
      if (persistenceScope) {
        writePersistedReadCache(persistenceScope, method, args, value);
      }
      trimReadCacheIfNeeded();
      _notifyCacheSubscribers(key);
      emitDataLoadMetric({
        requestId,
        resource: method,
        page,
        instanceId: metricInstanceId,
        instanceToken,
        source,
        phase: "success",
        elapsedMs,
        cacheHit: false,
      });
      return value;
    })
    .catch((error) => {
      const current = API_READ_CACHE.get(key);
      if (current?.inFlight === request) {
        API_READ_CACHE.delete(key);
      }
      emitDataLoadMetric({
        requestId,
        resource: method,
        page,
        instanceId: metricInstanceId,
        instanceToken,
        source,
        phase: "error",
        elapsedMs: Date.now() - startedAt,
        cacheHit: false,
        errorSummary: extractErrorText(error),
      });
      throw error;
    });
  API_READ_CACHE.set(key, {
    value: entry?.value,
    expiresAt: entry?.expiresAt ?? 0,
    optimisticUntil: entry?.optimisticUntil,
    inFlight: request as Promise<unknown>,
  });
  trimReadCacheIfNeeded();
  return request;
}

export function emitRemoteInvokeMetric(payload: Record<string, unknown>) {
  const line = `[metrics][remote_invoke] ${JSON.stringify(payload)}`;
  // fire-and-forget: metrics collection must not affect user flow
  void invoke("log_app_event", { message: line }).catch((error) => {
    if (import.meta.env.DEV) {
      console.warn("[dev ignored error] emitRemoteInvokeMetric", error);
    }
  });
}

export function logDevApiError(context: string, error: unknown, detail: Record<string, unknown> = {}): void {
  if (!import.meta.env.DEV) return;
  console.error(`[dev api error] ${context}`, {
    ...detail,
    error: extractErrorText(error),
  });
}

/** @internal Exported for testing only. */
export function shouldLogRemoteInvokeMetric(ok: boolean, elapsedMs: number): boolean {
  // Always log failures and slow calls; sample a small percentage of fast-success calls.
  if (!ok) return true;
  if (elapsedMs >= 1500) return true;
  return Math.random() < 0.05;
}

/**
 * Returns a unified API object that auto-dispatches to local or remote
 * based on the current instance context. Remote calls automatically
 * inject hostId and check connection state.
 */

// Expose cache clear for E2E perf tests — allows measuring IPC fetch + render
// rather than cache-hit render time.
if (typeof window !== "undefined") {
  (window as any).__TEST_CLEAR_READ_CACHE__ = () => {
    API_READ_CACHE.clear();
  };
}
