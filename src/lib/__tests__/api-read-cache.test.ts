import { describe, expect, test } from "bun:test";
import {
  hasGuidanceEmitted,
  buildCacheKey,
  shouldLogRemoteInvokeMetric,
} from "../api-read-cache";

describe("hasGuidanceEmitted", () => {
  test("returns false for null/undefined/string", () => {
    expect(hasGuidanceEmitted(null)).toBe(false);
    expect(hasGuidanceEmitted(undefined)).toBe(false);
    expect(hasGuidanceEmitted("error")).toBe(false);
  });

  test("returns false for object without flag", () => {
    expect(hasGuidanceEmitted({ message: "err" })).toBe(false);
  });

  test("returns true when _guidanceEmitted is truthy", () => {
    expect(hasGuidanceEmitted({ _guidanceEmitted: true })).toBe(true);
  });

  test("returns false when _guidanceEmitted is falsy", () => {
    expect(hasGuidanceEmitted({ _guidanceEmitted: false })).toBe(false);
    expect(hasGuidanceEmitted({ _guidanceEmitted: 0 })).toBe(false);
  });
});

describe("buildCacheKey", () => {
  test("builds deterministic key from instance+method", () => {
    const key = buildCacheKey("inst-1", "getStatus");
    expect(key).toContain("inst-1");
    expect(key).toContain("getStatus");
  });

  test("same inputs produce same key", () => {
    const a = buildCacheKey("i", "m", [1, "two"]);
    const b = buildCacheKey("i", "m", [1, "two"]);
    expect(a).toBe(b);
  });

  test("different args produce different keys", () => {
    const a = buildCacheKey("i", "m", [1]);
    const b = buildCacheKey("i", "m", [2]);
    expect(a).not.toBe(b);
  });
});

describe("shouldLogRemoteInvokeMetric", () => {
  test("returns true for failures", () => {
    expect(shouldLogRemoteInvokeMetric(false, 10)).toBe(true);
  });

  test("returns true for slow successes", () => {
    expect(shouldLogRemoteInvokeMetric(true, 10_000)).toBe(true);
  });

  test("returns false for fast successes", () => {
    expect(shouldLogRemoteInvokeMetric(true, 10)).toBe(false);
  });
});
