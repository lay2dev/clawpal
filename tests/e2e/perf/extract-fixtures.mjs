#!/usr/bin/env node
/**
 * Extract fixture data from the Docker OpenClaw container.
 * Runs `openclaw status --json` and related commands via SSH,
 * writes fixture JSON files for the IPC mock layer.
 */
import { execSync } from "node:child_process";
import { writeFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const FIXTURES_DIR = join(__dirname, "fixtures");
const SSH_PORT = process.env.CLAWPAL_PERF_SSH_PORT || "2299";
const SSH = `sshpass -p clawpal-perf-e2e ssh -o StrictHostKeyChecking=no -p ${SSH_PORT} root@localhost`;

mkdirSync(FIXTURES_DIR, { recursive: true });

function ssh(cmd) {
  try {
    return execSync(`${SSH} "${cmd}"`, { encoding: "utf-8", timeout: 15_000 }).trim();
  } catch (e) {
    console.error(`SSH command failed: ${cmd}`, e.message);
    return null;
  }
}

// Read raw config
const rawConfig = ssh("cat /root/.openclaw/openclaw.json");
if (rawConfig) {
  const config = JSON.parse(rawConfig);

  // Build configSnapshot
  const configSnapshot = {
    globalDefaultModel: config.defaults?.model ?? null,
    fallbackModels: config.defaults?.fallbackModels ?? [],
    agents: (config.agents?.list ?? []).map((a) => ({
      id: a.id,
      model: a.model ?? null,
      channels: [],
      online: false,
    })),
  };
  writeFileSync(join(FIXTURES_DIR, "configSnapshot.json"), JSON.stringify(configSnapshot, null, 2));

  // Build runtimeSnapshot (simulate)
  const runtimeSnapshot = {
    status: {
      healthy: true,
      activeAgents: configSnapshot.agents.length,
    },
    agents: configSnapshot.agents.map((a) => ({ ...a, online: true })),
    globalDefaultModel: configSnapshot.globalDefaultModel,
    fallbackModels: configSnapshot.fallbackModels,
  };
  writeFileSync(join(FIXTURES_DIR, "runtimeSnapshot.json"), JSON.stringify(runtimeSnapshot, null, 2));

  // statusExtra
  const versionRaw = ssh("openclaw --version 2>/dev/null || echo unknown");
  const statusExtra = {
    openclawVersion: versionRaw || "unknown",
  };
  writeFileSync(join(FIXTURES_DIR, "statusExtra.json"), JSON.stringify(statusExtra, null, 2));

  // modelProfiles
  const modelProfiles = Object.entries(config.models || {}).map(([id, m], i) => ({
    id,
    provider: m.provider,
    model: m.model,
    enabled: true,
  }));
  writeFileSync(join(FIXTURES_DIR, "modelProfiles.json"), JSON.stringify(modelProfiles, null, 2));
}

console.log("Fixtures extracted to", FIXTURES_DIR);
