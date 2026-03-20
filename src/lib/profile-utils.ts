/**
 * Utility functions for model profile credential handling.
 * Extracted from Settings.tsx for readability.
 */

export type ProfileForm = {
  id: string;
  provider: string;
  model: string;
  authRef: string;
  apiKey: string;
  useCustomUrl: boolean;
  baseUrl: string;
  enabled: boolean;
};

export type CredentialSource = "manual" | "env" | "oauth";

export function emptyForm(): ProfileForm {
  return { id: "", provider: "", model: "", authRef: "", apiKey: "", useCustomUrl: false, baseUrl: "", enabled: true };
}

export function normalizeOauthProvider(provider: string): string {
  const lower = provider.trim().toLowerCase();
  if (lower === "openai_codex" || lower === "github-copilot" || lower === "copilot") return "openai-codex";
  return lower;
}

export function providerUsesOAuthAuth(provider: string): boolean {
  return normalizeOauthProvider(provider) === "openai-codex";
}

export function defaultOauthAuthRef(provider: string): string {
  return normalizeOauthProvider(provider) === "openai-codex" ? "openai-codex:default" : "";
}

export function isEnvVarLikeAuthRef(authRef: string): boolean {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(authRef.trim());
}

export function defaultEnvAuthRef(provider: string): string {
  const normalized = normalizeOauthProvider(provider);
  if (!normalized) return "";
  if (normalized === "openai-codex") return "OPENAI_CODEX_TOKEN";
  const providerEnv = normalized.replace(/[^a-z0-9]+/g, "_").replace(/^_+|_+$/g, "").toUpperCase();
  return providerEnv ? `${providerEnv}_API_KEY` : "";
}

export function inferCredentialSource(provider: string, authRef: string): CredentialSource {
  const trimmed = authRef.trim();
  if (!trimmed) return providerUsesOAuthAuth(provider) ? "oauth" : "manual";
  if (providerUsesOAuthAuth(provider) && trimmed.toLowerCase().startsWith("openai-codex:")) return "oauth";
  return "env";
}

export function providerSupportsOptionalApiKey(provider: string): boolean {
  if (providerUsesOAuthAuth(provider)) return true;
  const lower = provider.trim().toLowerCase();
  return ["ollama", "lmstudio", "lm-studio", "localai", "vllm", "llamacpp", "llama.cpp"].includes(lower);
}
