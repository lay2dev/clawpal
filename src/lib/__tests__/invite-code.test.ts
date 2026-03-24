import { afterEach, beforeEach, describe, expect, test } from "bun:test";

import {
  deriveServerBaseUrlFromGatewayUrl,
  exchangeInviteCodeForApiKey,
  InviteCodeExchangeError,
} from "../invite-code";

describe("deriveServerBaseUrlFromGatewayUrl", () => {
  test("returns clawpal server default when gateway url is empty", () => {
    expect(deriveServerBaseUrlFromGatewayUrl("")).toBe("http://65.21.45.43:3040");
  });

  test("converts websocket gateway url to http origin", () => {
    expect(deriveServerBaseUrlFromGatewayUrl("ws://65.21.45.43:3040/ws")).toBe("http://65.21.45.43:3040");
    expect(deriveServerBaseUrlFromGatewayUrl("wss://server.example.com/ws")).toBe("https://server.example.com");
  });

  test("keeps http/https url origin", () => {
    expect(deriveServerBaseUrlFromGatewayUrl("https://server.example.com/path")).toBe("https://server.example.com");
  });
});

describe("exchangeInviteCodeForApiKey", () => {
  const originalFetch = globalThis.fetch;

  beforeEach(() => {
    globalThis.fetch = (async () => new Response("unexpected call", { status: 500 })) as typeof fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  test("returns api key on successful exchange", async () => {
    globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
      expect(String(input)).toBe("http://65.21.45.43:3040/api-keys/exchange");
      expect(init?.method).toBe("POST");
      expect(init?.headers).toEqual({ "content-type": "application/json" });
      expect(init?.body).toBe(JSON.stringify({ inviteCode: "invite-001" }));
      return new Response(JSON.stringify({ apiKey: "new-api-key" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }) as typeof fetch;

    await expect(exchangeInviteCodeForApiKey("invite-001", "")).resolves.toBe("new-api-key");
  });

  test("throws INVITE_CODE_REQUIRED for empty invite code", async () => {
    await expect(exchangeInviteCodeForApiKey("   ", "")).rejects.toMatchObject({
      code: "INVITE_CODE_REQUIRED",
    });
  });

  test("maps invalid invite code from server", async () => {
    globalThis.fetch = (async () =>
      new Response(JSON.stringify({ error: "invalid invite code" }), {
        status: 400,
        headers: { "content-type": "application/json" },
      })) as typeof fetch;

    await expect(exchangeInviteCodeForApiKey("bad-code", "")).rejects.toMatchObject({
      code: "INVALID_INVITE_CODE",
    });
  });

  test("maps internal server error from server", async () => {
    globalThis.fetch = (async () =>
      new Response(JSON.stringify({ error: "internal server error" }), {
        status: 500,
        headers: { "content-type": "application/json" },
      })) as typeof fetch;

    await expect(exchangeInviteCodeForApiKey("invite-001", "")).rejects.toMatchObject({
      code: "EXCHANGE_FAILED",
    });
  });

  test("wraps network failures", async () => {
    globalThis.fetch = (async () => {
      throw new Error("socket hang up");
    }) as typeof fetch;

    try {
      await exchangeInviteCodeForApiKey("invite-001", "");
      throw new Error("expected exchange to fail");
    } catch (error) {
      expect(error).toBeInstanceOf(InviteCodeExchangeError);
      expect((error as InviteCodeExchangeError).code).toBe("NETWORK_ERROR");
    }
  });
});
