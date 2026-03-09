import { describe, expect, test } from "bun:test";

import {
  shouldEnableInstanceLiveReads,
  shouldEnableLocalInstanceScope,
} from "../instance-availability";

describe("instance availability", () => {
  test("only enables a local instance scope when config exists and the CLI is available", () => {
    expect(shouldEnableLocalInstanceScope({ configExists: true, cliAvailable: true })).toBe(true);
    expect(shouldEnableLocalInstanceScope({ configExists: true, cliAvailable: false })).toBe(false);
    expect(shouldEnableLocalInstanceScope({ configExists: false, cliAvailable: true })).toBe(false);
  });

  test("blocks local live reads when no trusted persistence scope is available", () => {
    expect(shouldEnableInstanceLiveReads({
      instanceToken: 1,
      persistenceResolved: true,
      persistenceScope: null,
      isRemote: false,
    })).toBe(false);
  });

  test("allows remote live reads once the instance token is ready even before persistence is established", () => {
    expect(shouldEnableInstanceLiveReads({
      instanceToken: 1,
      persistenceResolved: true,
      persistenceScope: null,
      isRemote: true,
    })).toBe(true);
  });

  test("blocks all live reads until the instance context is resolved", () => {
    expect(shouldEnableInstanceLiveReads({
      instanceToken: 0,
      persistenceResolved: true,
      persistenceScope: "local",
      isRemote: false,
    })).toBe(false);

    expect(shouldEnableInstanceLiveReads({
      instanceToken: 1,
      persistenceResolved: false,
      persistenceScope: "local",
      isRemote: false,
    })).toBe(false);
  });
});
