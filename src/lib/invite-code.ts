const DEFAULT_CLAWPAL_SERVER_BASE_URL = "http://65.21.45.43:3040";

export type InviteCodeExchangeErrorCode =
  | "INVITE_CODE_REQUIRED"
  | "INVALID_INVITE_CODE"
  | "NETWORK_ERROR"
  | "EXCHANGE_FAILED";

export class InviteCodeExchangeError extends Error {
  code: InviteCodeExchangeErrorCode;

  constructor(code: InviteCodeExchangeErrorCode, message: string) {
    super(message);
    this.name = "InviteCodeExchangeError";
    this.code = code;
  }
}

export function deriveServerBaseUrlFromGatewayUrl(gatewayUrl: string): string {
  const trimmed = gatewayUrl.trim();
  if (!trimmed) return DEFAULT_CLAWPAL_SERVER_BASE_URL;

  try {
    const url = new URL(trimmed);
    if (url.protocol === "ws:") url.protocol = "http:";
    if (url.protocol === "wss:") url.protocol = "https:";
    return url.origin;
  } catch {
    return DEFAULT_CLAWPAL_SERVER_BASE_URL;
  }
}

export async function exchangeInviteCodeForApiKey(
  inviteCode: string,
  gatewayUrl: string,
): Promise<string> {
  const normalizedInviteCode = inviteCode.trim();
  if (!normalizedInviteCode) {
    throw new InviteCodeExchangeError("INVITE_CODE_REQUIRED", "inviteCode is required");
  }

  const serverBaseUrl = deriveServerBaseUrlFromGatewayUrl(gatewayUrl);
  const endpoint = `${serverBaseUrl}/api-keys/exchange`;
  let response: Response;
  try {
    response = await fetch(endpoint, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ inviteCode: normalizedInviteCode }),
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new InviteCodeExchangeError("NETWORK_ERROR", message);
  }

  let payload: unknown = null;
  try {
    payload = await response.json();
  } catch {
    payload = null;
  }

  if (!response.ok) {
    const errorText = payload && typeof payload === "object" && "error" in payload
      ? String((payload as { error: unknown }).error)
      : `HTTP ${response.status}`;
    if (response.status === 400 && errorText === "inviteCode is required") {
      throw new InviteCodeExchangeError("INVITE_CODE_REQUIRED", errorText);
    }
    if (response.status === 400 && errorText === "invalid invite code") {
      throw new InviteCodeExchangeError("INVALID_INVITE_CODE", errorText);
    }
    throw new InviteCodeExchangeError("EXCHANGE_FAILED", errorText);
  }

  const apiKey = payload && typeof payload === "object" && "apiKey" in payload
    ? String((payload as { apiKey: unknown }).apiKey ?? "")
    : "";
  if (!apiKey) {
    throw new InviteCodeExchangeError("EXCHANGE_FAILED", "apiKey missing in exchange response");
  }
  return apiKey;
}
