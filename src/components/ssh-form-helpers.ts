import type { SshConfigHostSuggestion, SshHost } from "@/lib/types";

export const SSH_CONFIG_MANUAL_ALIAS = "__manual__";

export function dedupeAndSortSshConfigHosts(
  sshConfigSuggestions: SshConfigHostSuggestion[],
): SshConfigHostSuggestion[] {
  const seen = new Set<string>();
  return [...sshConfigSuggestions]
    .filter((item) => {
      const key = item.hostAlias.trim();
      if (!key || seen.has(key)) return false;
      seen.add(key);
      return true;
    })
    .sort((a, b) => a.hostAlias.localeCompare(b.hostAlias, undefined, { sensitivity: "base" }));
}

export function resolveSshConfigPresetSelection(
  alias: string,
  filteredSshConfigHosts: SshConfigHostSuggestion[],
): {
  selectedSshConfigAlias: string;
  host?: string;
  username?: string;
  port?: string;
  keyPath?: string;
  password?: string;
  passphrase?: string;
  authMethod?: "ssh_config";
  label?: string;
} {
  if (alias === SSH_CONFIG_MANUAL_ALIAS) {
    return {
      selectedSshConfigAlias: SSH_CONFIG_MANUAL_ALIAS,
    };
  }

  const preset = filteredSshConfigHosts.find((item) => item.hostAlias === alias);
  if (!preset) {
    return {
      selectedSshConfigAlias: SSH_CONFIG_MANUAL_ALIAS,
    };
  }

  return {
    selectedSshConfigAlias: alias,
    host: preset.hostAlias,
    username: preset.user ?? "",
    port: String(preset.port ?? 22),
    keyPath: preset.identityFile ?? "",
    password: "",
    passphrase: "",
    authMethod: "ssh_config",
    label: preset.hostAlias,
  };
}

export function applySshConfigSuggestionToForm(
  alias: string,
  filteredSshConfigHosts: SshConfigHostSuggestion[],
  setters: {
    setSelectedSshConfigAlias: (value: string) => void;
    setHost: (value: string) => void;
    setUsername: (value: string) => void;
    setPort: (value: string) => void;
    setKeyPath: (value: string) => void;
    setPassword: (value: string) => void;
    setPassphrase: (value: string) => void;
    setAuthMethod: (value: "ssh_config") => void;
    setLabel: (value: string) => void;
  },
) {
  const nextSelection = resolveSshConfigPresetSelection(alias, filteredSshConfigHosts);
  setters.setSelectedSshConfigAlias(nextSelection.selectedSshConfigAlias);
  if (nextSelection.selectedSshConfigAlias === SSH_CONFIG_MANUAL_ALIAS) {
    return nextSelection;
  }

  setters.setHost(nextSelection.host ?? "");
  setters.setUsername(nextSelection.username ?? "");
  setters.setPort(nextSelection.port ?? "22");
  setters.setKeyPath(nextSelection.keyPath ?? "");
  setters.setPassword(nextSelection.password ?? "");
  setters.setPassphrase(nextSelection.passphrase ?? "");
  setters.setAuthMethod(nextSelection.authMethod ?? "ssh_config");
  setters.setLabel(nextSelection.label ?? "");
  return nextSelection;
}

export function buildSshFormSubmission(params: {
  host: string;
  port: string;
  username: string;
  authMethod: "ssh_config" | "key" | "password";
  keyPath: string;
  password: string;
  passphrase: string;
  label: string;
}): SshHost | null {
  const trimmedHost = params.host.trim();
  if (!trimmedHost || (params.authMethod === "password" && !params.password.length)) {
    return null;
  }

  return {
    id: "",
    label: params.label.trim() || trimmedHost,
    host: trimmedHost,
    port: parseInt(params.port, 10) || 22,
    username: params.username.trim(),
    authMethod: params.authMethod,
    keyPath: params.authMethod === "key" ? params.keyPath.trim() : undefined,
    password: params.authMethod === "password" ? params.password : undefined,
    passphrase: params.authMethod !== "password" && params.passphrase ? params.passphrase : undefined,
  };
}

export function submitSshForm(params: {
  invokeId: string;
  host: string;
  port: string;
  username: string;
  authMethod: "ssh_config" | "key" | "password";
  keyPath: string;
  password: string;
  passphrase: string;
  label: string;
  onSubmit: (invokeId: string, host: SshHost) => void;
}): boolean {
  const payload = buildSshFormSubmission(params);
  if (!payload) return false;
  params.onSubmit(params.invokeId, payload);
  return true;
}

export function formatSshConfigSuggestionLabel(item: SshConfigHostSuggestion): string {
  return `${item.hostAlias}${item.hostName ? ` (${item.hostName})` : ""}${item.user ? ` • ${item.user}` : ""}${item.port && item.port !== 22 ? `:${item.port}` : ""}`;
}
