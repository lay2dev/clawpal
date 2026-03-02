import { describe, expect, test } from "bun:test";

import {
  SSH_PASSPHRASE_RETRY_HINT,
  SSH_PASSPHRASE_REJECT_HINT,
  SSH_NO_KEY_HINT,
  buildSshPassphraseConnectErrorMessage,
  buildSshPassphraseCancelMessage,
  SSH_PUBLIC_KEY_PERMISSION_HINT,
} from "../sshConnectErrors";

describe("sshConnectErrors", () => {
  test("classifies passphrase-required error", () => {
    expect(SSH_PASSPHRASE_RETRY_HINT.test("The key is encrypted.")).toBe(true);
  });

  test("classifies wrong passphrase", () => {
    expect(SSH_PASSPHRASE_REJECT_HINT.test("bad decrypt")).toBe(true);
  });

  test("classifies missing key path error", () => {
    expect(SSH_NO_KEY_HINT.test("Could not open /Users/foo/.ssh/hetzner")).toBe(true);
  });

  test("maps passphrase connect error to clear user message", () => {
    const host = "hetzner";
    const message = buildSshPassphraseConnectErrorMessage("passphrase authentication failed: Bad decrypt", host);
    expect(message).toContain(`host: ${host}`);
    expect(message).toContain("SSH 口令校验失败");
  });

  test("maps key-missing connect error to clear user message", () => {
    const host = "hetzner";
    const message = buildSshPassphraseConnectErrorMessage("Could not open key file /Users/foo/.ssh/id_rsa", host);
    expect(message).toContain(`host: ${host}`);
    expect(message).toContain("未找到可用私钥文件");
  });

  test("maps permission denied connect error to clear user message", () => {
    const host = "hetzner";
    const message = buildSshPassphraseConnectErrorMessage("permission denied: public key authentication failed", host);
    expect(message).toContain(`host: ${host}`);
    expect(message).toContain("SSH 认证失败");
    expect(message).toContain("authorized_keys");
  });

  test("returns null for unrelated message", () => {
    expect(
      buildSshPassphraseConnectErrorMessage("ssh connect timeout after 10s", "hetzner"),
    ).toBeNull();
  });

  test("returns cancel message with host label", () => {
    expect(buildSshPassphraseCancelMessage("hetzner")).toContain("hetzner");
  });

  test("exposes permission-denied hint", () => {
    expect(SSH_PUBLIC_KEY_PERMISSION_HINT.test("public key authentication failed")).toBe(true);
  });
});
