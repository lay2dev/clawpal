import { describe, expect, test } from "bun:test";

import {
  buildFailedSshConnectionProfileFromProgress,
  buildSshProbeProgressLine,
  formatSshConnectionLatency,
  getSshConnectionStageMetrics,
  mergeSshProbeStageMetric,
  shouldMarkSshProbeAsChecked,
  shouldAutoProbeSshConnectionProfile,
  shouldDeferInteractiveSshAutoProbe,
} from "../sshConnectionProfile";

describe("sshConnectionProfile helpers", () => {
  test("formats slow SSH latency in seconds", () => {
    expect(formatSshConnectionLatency(2420)).toBe("2.42 s");
  });

  test("auto-probes only untouched hosts", () => {
    expect(shouldAutoProbeSshConnectionProfile({
      checked: false,
      checking: false,
      hasProfile: false,
      deferredInteractive: false,
    })).toBe(true);

    expect(shouldAutoProbeSshConnectionProfile({
      checked: true,
      checking: false,
      hasProfile: false,
      deferredInteractive: false,
    })).toBe(false);

    expect(shouldAutoProbeSshConnectionProfile({
      checked: false,
      checking: true,
      hasProfile: false,
      deferredInteractive: false,
    })).toBe(false);

    expect(shouldAutoProbeSshConnectionProfile({
      checked: false,
      checking: false,
      hasProfile: false,
      deferredInteractive: true,
    })).toBe(false);
  });

  test("defers auto-probe when SSH needs interactive passphrase input", () => {
    expect(shouldDeferInteractiveSshAutoProbe("The key is encrypted")).toBe(true);
    expect(shouldDeferInteractiveSshAutoProbe("ssh connect timeout after 10s")).toBe(false);
  });

  test("does not mark interactive probe results as checked", () => {
    expect(shouldMarkSshProbeAsChecked({
      probeStatus: "interactive_required",
      status: { healthy: false, activeAgents: 0, sshDiagnostic: null },
      connectLatencyMs: 180,
      gatewayLatencyMs: 0,
      configLatencyMs: 0,
      versionLatencyMs: 0,
      totalLatencyMs: 180,
      quality: "unknown",
      qualityScore: 0,
      bottleneck: { stage: "connect", latencyMs: 180 },
    })).toBe(false);
  });

  test("backfills legacy stage metrics with agents timing", () => {
    expect(getSshConnectionStageMetrics({
      status: { healthy: true, activeAgents: 4, sshDiagnostic: null },
      connectLatencyMs: 10,
      gatewayLatencyMs: 20,
      configLatencyMs: 30,
      agentsLatencyMs: 40,
      versionLatencyMs: 50,
      totalLatencyMs: 150,
      quality: "good",
      qualityScore: 84,
      bottleneck: { stage: "agents", latencyMs: 40 },
    })).toEqual([
      { key: "connect", latencyMs: 10, status: "ok" },
      { key: "gateway", latencyMs: 20, status: "ok" },
      { key: "config", latencyMs: 30, status: "ok" },
      { key: "agents", latencyMs: 40, status: "ok" },
      { key: "version", latencyMs: 50, status: "ok" },
    ]);
  });

  test("formats a single-line SSH probe progress message", () => {
    const line = buildSshProbeProgressLine(
      {
        hostId: "ssh:lay2dev",
        requestId: "req-1",
        stage: "agents",
        phase: "success",
      },
      (key, options) => {
        if (key === "start.sshStage.agents") return "Agents fetch";
        if (key === "start.sshProbe.success") return `${options?.stage} succeeded`;
        return key;
      },
    );

    expect(line).toBe("Agents fetch succeeded");
  });

  test("merges probe stage metrics as progress events arrive", () => {
    const metrics = mergeSshProbeStageMetric(
      [{ key: "connect", latencyMs: 0, status: "reused", note: "Session reused" }],
      {
        hostId: "ssh:lay2dev",
        requestId: "req-1",
        stage: "gateway",
        phase: "success",
        latencyMs: 62,
      },
    );

    expect(metrics).toEqual([
      { key: "connect", latencyMs: 0, status: "reused", note: "Session reused" },
      { key: "gateway", latencyMs: 62, status: "ok", note: null },
    ]);
  });

  test("builds a failed SSH profile from partial progress", () => {
    const profile = buildFailedSshConnectionProfileFromProgress({
      errorSummary: "Agents fetch timed out",
      failingStage: "agents",
      progressStages: [
        { key: "connect", latencyMs: 0, status: "reused", note: "Session reused" },
        { key: "gateway", latencyMs: 45, status: "ok" },
        { key: "config", latencyMs: 110, status: "ok" },
      ],
    });

    expect(profile.probeStatus).toBe("failed");
    expect(profile.bottleneck).toEqual({ stage: "config", latencyMs: 110 });
    expect(profile.stages).toEqual([
      { key: "connect", latencyMs: 0, status: "reused", note: "Session reused" },
      { key: "gateway", latencyMs: 45, status: "ok" },
      { key: "config", latencyMs: 110, status: "ok" },
      { key: "agents", latencyMs: 0, status: "failed", note: "Agents fetch timed out" },
      { key: "version", latencyMs: 0, status: "not_run" },
    ]);
  });
});
