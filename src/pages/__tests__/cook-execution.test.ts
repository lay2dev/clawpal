import { describe, expect, test } from "bun:test";

import {
  buildCookExecutionSpec,
  markCookFailure,
  markCookStatuses,
} from "../cook-execution";

describe("cook execution helpers", () => {
  test("builds a remote execution target from instance context", () => {
    const spec = buildCookExecutionSpec(
      {
        apiVersion: "strategy.platform/v1",
        kind: "ExecutionSpec",
        metadata: {},
        source: {},
        target: {},
        execution: { kind: "job" },
        capabilities: { usedCapabilities: [] },
        resources: { claims: [] },
        secrets: { bindings: [] },
        desiredState: {},
        actions: [],
        outputs: [],
      },
      {
        instanceId: "ssh:prod-a",
        isRemote: true,
        isDocker: false,
      },
    );

    expect(spec.target).toEqual({
      kind: "remote_ssh",
      hostId: "ssh:prod-a",
    });
  });

  test("marks non-skipped steps with the requested execution state", () => {
    expect(markCookStatuses(["pending", "skipped", "pending"], "running")).toEqual([
      "running",
      "skipped",
      "running",
    ]);
  });

  test("marks the first executable step failed and leaves remaining ones pending", () => {
    expect(markCookFailure(["running", "running", "skipped"])).toEqual([
      "failed",
      "pending",
      "skipped",
    ]);
  });
});
