import type { DockerInstance } from "./types";

export const LEGACY_DOCKER_INSTANCES_KEY = "clawpal_docker_instances";
export const DEFAULT_DOCKER_OPENCLAW_HOME = "~/.clawpal/docker-local";
export const DEFAULT_DOCKER_CLAWPAL_DATA_DIR = "~/.clawpal/docker-local/data";
export const DEFAULT_DOCKER_INSTANCE_ID = "docker:local";

export function sanitizeDockerPathSuffix(raw: string): string {
  const lowered = raw.toLowerCase().replace(/[^a-z0-9_-]/g, "");
  const trimmed = lowered.replace(/^[-_]+|[-_]+$/g, "");
  return trimmed || "docker-local";
}

export function deriveDockerPaths(instanceId: string): { openclawHome: string; clawpalDataDir: string } {
  if (instanceId === DEFAULT_DOCKER_INSTANCE_ID) {
    return {
      openclawHome: DEFAULT_DOCKER_OPENCLAW_HOME,
      clawpalDataDir: DEFAULT_DOCKER_CLAWPAL_DATA_DIR,
    };
  }
  const suffixRaw = instanceId.startsWith("docker:") ? instanceId.slice(7) : instanceId;
  const suffix = suffixRaw === "local"
    ? "docker-local"
    : suffixRaw.startsWith("docker-")
      ? sanitizeDockerPathSuffix(suffixRaw)
      : `docker-${sanitizeDockerPathSuffix(suffixRaw)}`;
  const openclawHome = `~/.clawpal/${suffix}`;
  return {
    openclawHome,
    clawpalDataDir: `${openclawHome}/data`,
  };
}

export function deriveDockerLabel(instanceId: string): string {
  if (instanceId === DEFAULT_DOCKER_INSTANCE_ID) return "docker-local";
  const suffix = instanceId.startsWith("docker:") ? instanceId.slice(7) : instanceId;
  const match = suffix.match(/^local-(\d+)$/);
  if (match) return `docker-local-${match[1]}`;
  return suffix.startsWith("docker-") ? suffix : `docker-${suffix}`;
}

export function hashInstanceToken(raw: string): number {
  let hash = 2166136261;
  for (let i = 0; i < raw.length; i += 1) {
    hash ^= raw.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

export function normalizeDockerInstance(instance: DockerInstance): DockerInstance {
  const fallback = deriveDockerPaths(instance.id);
  return {
    ...instance,
    label: instance.label?.trim() || deriveDockerLabel(instance.id),
    openclawHome: instance.openclawHome || fallback.openclawHome,
    clawpalDataDir: instance.clawpalDataDir || fallback.clawpalDataDir,
  };
}
