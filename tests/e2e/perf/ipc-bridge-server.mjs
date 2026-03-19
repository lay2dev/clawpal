#!/usr/bin/env node
/**
 * IPC Bridge Server — serves real OpenClaw config data from Docker container.
 * Reads config once via SSH, then serves from in-memory cache.
 */
import { createServer } from "node:http";
import { execSync } from "node:child_process";

const PORT = parseInt(process.env.BRIDGE_PORT || "3399", 10);
const SSH_PORT = process.env.CLAWPAL_PERF_SSH_PORT || "2299";
const SSH_PREFIX = `sshpass -p clawpal-perf-e2e ssh -o StrictHostKeyChecking=no -p ${SSH_PORT} root@localhost`;

console.log("Pre-fetching data from Docker OpenClaw...");
const startMs = Date.now();

// Single SSH call: just read the config file (fast, no CLI startup overhead)
let rawConfig = "{}";
try {
  rawConfig = execSync(
    `${SSH_PREFIX} cat /root/.openclaw/openclaw.json`,
    { encoding: "utf-8", timeout: 15_000 },
  ).trim();
} catch (e) {
  console.warn("Failed to read config via SSH:", e.message);
}

const cfg = JSON.parse(rawConfig || "{}");
console.log(`Pre-fetch done in ${Date.now() - startMs}ms`);

// Build all responses from config
const agents = (cfg.agents?.list ?? []).map((a) => ({
  id: a.id, model: a.model ?? null, channels: [], online: false,
}));

const modelsObj = cfg.agents?.defaults?.models || {};
const modelProfiles = Object.entries(modelsObj).map(([id, m]) => {
  const parts = id.split("/");
  return { id, provider: m?.provider || parts[0], model: m?.model || parts.slice(1).join("/") || id, enabled: true };
});

if (agents.length === 0 && modelProfiles.length === 0) {
  console.error("FATAL: Config has no agents or models. Raw config:", rawConfig.slice(0, 200));
  process.exit(1);
}

const configSnapshot = {
  globalDefaultModel: cfg.agents?.defaults?.model?.primary ?? cfg.agents?.defaults?.model ?? null,
  fallbackModels: cfg.agents?.defaults?.model?.fallbacks ?? [],
  agents,
};

const runtimeSnapshot = {
  status: { healthy: true, activeAgents: agents.length },
  agents: agents.map((a) => ({ ...a, online: true })),
  globalDefaultModel: configSnapshot.globalDefaultModel,
  fallbackModels: configSnapshot.fallbackModels,
};

const cache = {
  get_instance_config_snapshot: configSnapshot,
  get_instance_runtime_snapshot: runtimeSnapshot,
  get_status_extra: { openclawVersion: "perf-test" },
  get_status_light: { healthy: true, activeAgents: agents.length },
  list_model_profiles: modelProfiles,
  list_agents_overview: agents,
  get_channels_config_snapshot: { channels: [], bindings: [] },
  get_channels_runtime_snapshot: { channels: [], bindings: [], agents: [] },
  get_cron_config_snapshot: { jobs: [] },
  get_cron_runtime_snapshot: { jobs: [], watchdog: null },
  get_rescue_bot_status: {
    action: "status", profile: "rescue", mainPort: 18789, rescuePort: 19789,
    minRecommendedPort: 19789, configured: false, active: false,
    runtimeState: "unconfigured", wasAlreadyConfigured: false, commands: [],
  },
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

const server = createServer(async (req, res) => {
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Methods", "POST, OPTIONS");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type");
  if (req.method === "OPTIONS") { res.writeHead(204); return res.end(); }
  if (req.method !== "POST" || req.url !== "/invoke") { res.writeHead(404); return res.end("Not found"); }
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const { cmd } = JSON.parse(Buffer.concat(chunks).toString());
  const result = cmd in cache ? cache[cmd] : null;
  res.writeHead(200, { "Content-Type": "application/json" });
  res.end(JSON.stringify({ ok: true, result }));
});

server.listen(PORT, () => {
  console.log(`IPC Bridge Server listening on http://localhost:${PORT} (${agents.length} agents, ${modelProfiles.length} models)`);
});
