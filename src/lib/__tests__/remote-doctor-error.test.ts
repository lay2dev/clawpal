import { describe, expect, test } from "bun:test";

import { formatRemoteDoctorErrorMessage } from "../remote-doctor-error";

describe("formatRemoteDoctorErrorMessage", () => {
  test("adds an api-key-focused hint for invalid-token handshake failures", () => {
    expect(
      formatRemoteDoctorErrorMessage(
        "Connection lost while waiting for response: server closed (close code 1008: invalid token)",
      ),
    ).toContain("Remote Doctor API key");
  });

  test("surfaces invalid api key handshake failures explicitly", () => {
    expect(
      formatRemoteDoctorErrorMessage(
        "Remote Doctor gateway connect failed: Connection lost while waiting for response: server closed (close code 1008: invalid api key)",
      ),
    ).toContain("Re-save the invite code in Settings");
  });

  test("keeps unrelated errors unchanged", () => {
    expect(formatRemoteDoctorErrorMessage("Request timed out")).toBe("Request timed out");
  });
});
