#!/usr/bin/env node
/**
 * IPC Bridge Server — proxies Tauri IPC commands to a real OpenClaw instance
 * running in Docker via SSH. Two modes:
 *
 * 1. LIVE mode (default): Each /invoke request triggers a real SSH command
 *    to measure actual IPC round-trip latency.
 * 2. CACHED mode (BRIDGE_MODE=cached): Pre-fetches once, serves from memory.
 *    Only measures React render time, not IPC latency.
 *
 * Commands that map to OpenClaw CLI are executed live via SSH.
 * Commands without a CLI mapping return sensible defaults.
 */
import { createServer } from "node:http";
import { execSync } from "node:child_process";

const PORT = parseInt(process.env.BRIDGE_PORT || "3399", 10);
const SSH_PORT = process.env.CLAWPAL_PERF_SSH_PORT || "2299";
const SSH_PREFIX = `sshpass -p clawpal-perf-e2e ssh -o StrictHostKeyChecking=no -o ConnectTimeout=5 -p ${SSH_PORT} root@localhost`;
const MODE = process.env.BRIDGE_MODE || "live";

function ssh(cmd, timeoutMs = 10_000) {
  try {
    const escaped = cmd.replace(/'/g, "'\\''");
    return execSync(`${SSH_PREFIX} '${escaped}'`, {
      encoding: "utf-8",
      timeout: timeoutMs,
    }).trim();
  } catch {
    return null;
  }
}

function parseJson(raw) {
  if (!raw) return null;
  try { return JSON.parse(raw); } catch { return null; }
}

// ── Pre-fetch config once (needed for both modes) ──────────────────
console.log("Pre-fetching config from Docker OpenClaw...");
const startMs = Date.now();
const rawConfig = ssh("cat /root/.openclaw/openclaw.json") || "{}";
const cfg = parseJson(rawConfig) || {};
console.log(`Pre-fetch done in ${Date.now() - startMs}ms`);

const agents = (cfg.agents?.list ?? []).map((a) => ({
  id: a.id, model: a.model ?? null, channels: [], online: false,
}));
const modelsObj = cfg.agents?.defaults?.models || {};
const modelProfiles = Object.entries(modelsObj).map(([id, m]) => {
  const parts = id.split("/");
  return { id, provider: m?.provider || parts[0], model: m?.model || parts.slice(1).join("/") || id, enabled: true };
});

if (agents.length === 0 && modelProfiles.length === 0) {
  console.error("FATAL: Config has no agents or models.");
  process.exit(1);
}

// ── Live SSH command mapping ───────────────────────────────────────
// Maps IPC commands → SSH commands that return equivalent data
const LIVE_COMMANDS = {
  get_instance_runtime_snapshot: () => {
    const statusRaw = ssh("openclaw status --json");
    const status = parseJson(statusRaw);
    return {
      status: status ? { healthy: true, activeAgents: agents.length } : { healthy: false },
      agents: agents.map((a) => ({ ...a, online: true })),
      globalDefaultModel: cfg.agents?.defaults?.model?.primary ?? cfg.agents?.defaults?.model ?? null,
      fallbackModels: cfg.agents?.defaults?.model?.fallbacks ?? [],
    };
  },
  get_instance_config_snapshot: () => {
    const raw = ssh("cat /root/.openclaw/openclaw.json");
    const c = parseJson(raw) || cfg;
    return {
      globalDefaultModel: c.agents?.defaults?.model?.primary ?? c.agents?.defaults?.model ?? null,
      fallbackModels: c.agents?.defaults?.model?.fallbacks ?? [],
      agents: (c.agents?.list ?? []).map((a) => ({ id: a.id, model: a.model ?? null, channels: [], online: false })),
    };
  },
  get_status_extra: () => {
    const ver = ssh("openclaw --version 2>/dev/null") || "unknown";
    return { openclawVersion: ver };
  },
  get_status_light: () => {
    const statusRaw = ssh("openclaw status --json");
    return statusRaw ? { healthy: true, activeAgents: agents.length } : { healthy: false, activeAgents: 0 };
  },
  list_model_profiles: () => {
    const modRaw = ssh("openclaw config get agents.defaults.models --json");
    const mods = parseJson(modRaw) || modelsObj;
    return Object.entries(mods).map(([id, m]) => {
      const parts = id.split("/");
      return { id, provider: m?.provider || parts[0], model: m?.model || parts.slice(1).join("/") || id, enabled: true };
    });
  },
  list_agents_overview: () => {
    const raw = ssh("openclaw agents list --json");
    const parsed = parseJson(raw);
    return parsed || agents;
  },
};

// ── Cached fallbacks (for commands without SSH mapping) ────────────
const CACHED = {
  get_channels_config_snapshot: { channels: [], bindings: [] },
  get_channels_runtime_snapshot: { channels: [], bindings: [], agents: [] },
  get_cron_config_snapshot: { jobs: [] },
  get_cron_runtime_snapshot: { jobs: [], watchdog: null },
  get_rescue_bot_status: { action: "status", configured: false, active: false, runtimeState: "unconfigured" },
  queued_commands_count: 0,
  check_openclaw_update: { upgradeAvailable: false, latestVersion: null },
  log_app_event: true,
  get_app_preferences: {},
  get_bug_report_settings: {},
  get_bug_report_stats: {},
  ensure_access_profile: {},
  get_cached_model_catalog: [],
  list_recipes: [],
  install_list_methods: [],
  list_ssh_hosts: [],
  local_openclaw_config_exists: true,
  local_openclaw_cli_available: true,
  read_raw_config: rawConfig,
  get_system_status: { platform: "linux", arch: "x64" },
  list_channels_minimal: [],
  list_bindings: [],
  list_discord_guild_channels: [],
  get_watchdog_status: { alive: false, deployed: false },
  list_cron_jobs: [],
  list_history: { items: [] },
  list_session_files: [],
  list_backups: [],
  migrate_legacy_instances: null,
  list_registered_instances: [{ id: "local", instanceType: "local", label: "Local", createdAt: Date.now() }],
  discover_local_instances: [],
  list_ssh_config_hosts: [],
  set_active_openclaw_home: null,
  set_active_clawpal_data_dir: null,
  precheck_registry: { ok: true },
  precheck_transport: { ok: true },
  precheck_instance: { ok: true },
  precheck_auth: { ok: true },
  connect_local_instance: null,
  ssh_status: { connected: false },
  record_install_experience: null,
};

// In cached mode, pre-compute live command results too
const CACHED_LIVE = {};
if (MODE === "cached") {
  for (const [cmd, fn] of Object.entries(LIVE_COMMANDS)) {
    CACHED_LIVE[cmd] = fn();
  }
}

// ── HTTP server ────────────────────────────────────────────────────
const server = createServer(async (req, res) => {
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Methods", "POST, OPTIONS");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type");
  if (req.method === "OPTIONS") { res.writeHead(204); return res.end(); }
  if (req.method !== "POST" || req.url !== "/invoke") { res.writeHead(404); return res.end("Not found"); }

  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const { cmd } = JSON.parse(Buffer.concat(chunks).toString());

  let result;
  if (MODE === "live" && cmd in LIVE_COMMANDS) {
    const t0 = Date.now();
    result = LIVE_COMMANDS[cmd]();
    console.log(`[live] ${cmd} → ${Date.now() - t0}ms`);
  } else if (MODE === "cached" && cmd in CACHED_LIVE) {
    result = CACHED_LIVE[cmd];
  } else {
    result = CACHED[cmd] ?? null;
  }

  res.writeHead(200, { "Content-Type": "application/json" });
  res.end(JSON.stringify({ ok: true, result }));
});

server.listen(PORT, () => {
  console.log(`IPC Bridge Server listening on http://localhost:${PORT} (mode=${MODE}, ${agents.length} agents, ${modelProfiles.length} models)`);
});
