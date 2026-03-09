import { describe, expect, test } from "bun:test";

import { resolveInstallHubToastError } from "../install-hub-toast";

describe("resolveInstallHubToastError", () => {
  test("returns a toast for a new error", () => {
    expect(resolveInstallHubToastError({
      error: "Connection failed",
      previousError: null,
    })).toEqual({
      nextError: "Connection failed",
      toastMessage: "Connection failed",
    });
  });

  test("suppresses duplicate errors", () => {
    expect(resolveInstallHubToastError({
      error: "Connection failed",
      previousError: "Connection failed",
    })).toEqual({
      nextError: "Connection failed",
      toastMessage: null,
    });
  });

  test("ignores filtered install hub errors", () => {
    expect(resolveInstallHubToastError({
      error: "Auto-approve requires confirmation",
      previousError: null,
      ignoredSubstrings: ["Auto-approve"],
    })).toEqual({
      nextError: "Auto-approve requires confirmation",
      toastMessage: null,
    });
  });

  test("clears tracked error when the message disappears", () => {
    expect(resolveInstallHubToastError({
      error: "   ",
      previousError: "Connection failed",
    })).toEqual({
      nextError: null,
      toastMessage: null,
    });
  });
});
