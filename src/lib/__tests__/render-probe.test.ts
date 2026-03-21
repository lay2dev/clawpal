import { beforeEach, describe, expect, test } from "bun:test";
import { RenderProbe } from "../render-probe";

// Minimal performance mock for bun test environment
if (typeof performance === "undefined" || !performance.mark) {
  (globalThis as any).performance = {
    now: () => Date.now(),
    mark: () => {},
  };
}

describe("RenderProbe", () => {
  beforeEach(() => {
    (globalThis as any).window = {};
  });

  test("records first hit for a label", () => {
    const probe = new RenderProbe("test-page");
    probe.hit("status");
    const snap = probe.snapshot();
    expect(snap.status).toBeTypeOf("number");
    expect(snap.status).toBeGreaterThanOrEqual(0);
  });

  test("ignores duplicate hits for same label", () => {
    const probe = new RenderProbe("test-page");
    probe.hit("status");
    const first = probe.snapshot().status;
    // Wait a tiny bit to ensure performance.now() advances
    const start = performance.now();
    while (performance.now() - start < 2) { /* spin */ }
    probe.hit("status");
    expect(probe.snapshot().status).toBe(first);
  });

  test("tracks multiple labels independently", () => {
    const probe = new RenderProbe("test-page");
    probe.hit("status");
    probe.hit("agents");
    probe.hit("models");
    const snap = probe.snapshot();
    expect(Object.keys(snap)).toContain("status");
    expect(Object.keys(snap)).toContain("agents");
    expect(Object.keys(snap)).toContain("models");
  });

  test("settled() is an alias for hit('settled')", () => {
    const probe = new RenderProbe("test-page");
    probe.settled();
    expect(probe.snapshot().settled).toBeTypeOf("number");
  });

  test("exposes snapshot on window.__RENDER_PROBES__", () => {
    const probe = new RenderProbe("home");
    probe.hit("status");
    const probes = (globalThis as any).window.__RENDER_PROBES__;
    expect(probes).toBeDefined();
    expect(probes.home).toBeDefined();
    expect(probes.home.status).toBeTypeOf("number");
  });

  test("snapshot returns a copy (not a mutable reference)", () => {
    const probe = new RenderProbe("test-page");
    probe.hit("a");
    const snap1 = probe.snapshot();
    probe.hit("b");
    const snap2 = probe.snapshot();
    expect(snap1).not.toHaveProperty("b");
    expect(snap2).toHaveProperty("b");
  });
});
