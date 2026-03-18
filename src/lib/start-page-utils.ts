/**
 * Docker instance path derivation and normalization utilities.
 * Extracted from StartPage.tsx for readability.
 */

const DEFAULT_DOCKER_OPENCLAW_HOME = "~/.openclaw";
const DEFAULT_DOCKER_CLAWPAL_DATA_DIR = "~/.local/share/clawpal";

export function deriveDockerPaths(instanceId: string): { openclawHome: string; clawpalDataDir: string } {
  if (instanceId === "docker:local") {
    return { openclawHome: DEFAULT_DOCKER_OPENCLAW_HOME, clawpalDataDir: DEFAULT_DOCKER_CLAWPAL_DATA_DIR };
  }
  const suffixRaw = instanceId.startsWith("docker:") ? instanceId.slice(7) : instanceId;
  const suffix = suffixRaw === "local"
    ? "docker-local"
    : suffixRaw.startsWith("docker-") ? suffixRaw : `docker-${suffixRaw || "local"}`;
  const openclawHome = `~/.clawpal/${suffix}`;
  return { openclawHome, clawpalDataDir: `${openclawHome}/data` };
}

export function normalizePathForCompare(raw: string): string {
  const trimmed = raw.trim().replace(/\\/g, "/");
  return trimmed ? trimmed.replace(/\/+$/, "") : "";
}

export function dockerPathKey(raw: string): string {
  const normalized = normalizePathForCompare(raw);
  if (!normalized) return "";
  const segments = normalized.split("/").filter(Boolean);
  const clawpalIdx = segments.lastIndexOf(".clawpal");
  if (clawpalIdx >= 0 && clawpalIdx + 1 < segments.length) {
    const dir = segments[clawpalIdx + 1];
    if (dir.startsWith("docker-")) return `docker-dir:${dir.toLowerCase()}`;
  }
  const last = segments[segments.length - 1] || "";
  if (last.startsWith("docker-")) return `docker-dir:${last.toLowerCase()}`;
  return `path:${normalized.toLowerCase()}`;
}

export function dockerIdKey(rawId: string): string {
  if (!rawId.startsWith("docker:")) return "";
  let slug = rawId.slice("docker:".length).trim().toLowerCase();
  if (!slug) slug = "local";
  if (slug.startsWith("docker-")) slug = slug.slice("docker-".length);
  return `docker-id:${slug}`;
}
