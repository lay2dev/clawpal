#!/usr/bin/env node
/**
 * IPC Bridge Server — proxies Tauri invoke() calls to a real OpenClaw
 * instance running in Docker via SSH.
 *
 * Pre-fetches all data at startup via SSH, then serves from in-memory cache.
 * This avoids per-invoke SSH overhead while still using real OpenClaw data.
 */
import { createServer } from "node:http";
import { execSync } from "node:child_process";

const PORT = parseInt(process.env.BRIDGE_PORT || "3399", 10);
const SSH_PORT = process.env.CLAWPAL_PERF_SSH_PORT || "2299";
const SSH_PREFIX = `sshpass -p clawpal-perf-e2e ssh -o StrictHostKeyChecking=no -p ${SSH_PORT} root@localhost`;

function ssh(cmd, timeoutMs = 15_000) {
  try {
    return execSync(`${SSH_PREFIX} ${JSON.stringify(cmd)}`, {
      encoding: "utf-8",
      timeout: timeoutMs,
    }).trim();
  } catch {
    return null;
  }
}

function parseCliJson(raw) {
  if (!raw) return null;
  const start = raw.search(/[{\[]/);
  if (start === -1) return null;
  try {
    return JSON.parse(raw.slice(start));
  } catch {
    return null;
  }
}

// ---- Pre-fetch all data at startup ----
console.log("Pre-fetching data from Docker OpenClaw...");
const startMs = Date.now();

// Single SSH call to fetch all data at once (avoids 6× SSH connection overhead)
const batchCmd = [
  'echo "---CONFIG---"',
  'cat /root/.openclaw/openclaw.json 2>/dev/null || echo "{}"',
  'echo "---STATUS---"',
  'openclaw status --json 2>/dev/null || echo "null"',
  'echo "---AGENTS---"',
  'openclaw agents list --json 2>/dev/null || echo "null"',
  'echo "---MODELS---"',
  'openclaw config get agents.defaults.models --json 2>/dev/null || echo "null"',
  'echo "---VERSION---"',
  'openclaw --version 2>/dev/null || echo "unknown"',
].join("; ");

const batchRaw = ssh(batchCmd, 30_000) || "";

function extractSection(raw, marker) {
  const idx = raw.indexOf(marker);
  if (idx === -1) return null;
  const start = idx + marker.length;
  // Find next marker or end
  const markers = ["---CONFIG---", "---STATUS---", "---AGENTS---", "---MODELS---", "---VERSION---"];
  let end = raw.length;
  for (const m of markers) {
    const mi = raw.indexOf(m, start);
    if (mi !== -1 && mi < end) end = mi;
  }
  return raw.slice(start, end).trim();
}

const rawConfig = extractSection(batchRaw, "---CONFIG---") || "{}";
const cfg = parseCliJson(rawConfig) || {};
const statusRaw = extractSection(batchRaw, "---STATUS---");
const agentsRaw = extractSection(batchRaw, "---AGENTS---");
const modelsRaw = extractSection(batchRaw, "---MODELS---");
const versionRaw = extractSection(batchRaw, "---VERSION---") || "unknown";
const channelsRaw = null;
const cronRaw = null;

const status = parseCliJson(statusRaw);
const agentsList = parseCliJson(agentsRaw);
const models = parseCliJson(modelsRaw);
const channels = parseCliJson(channelsRaw);
const cron = parseCliJson(cronRaw);

// Build cached responses
const agents = (cfg.agents?.list ?? []).map((a) => ({
  id: a.id,
  model: a.model ?? null,
  channels: [],
  online: false,
}));

const configSnapshot = {
  globalDefaultModel: cfg.agents?.defaults?.model?.primary ?? cfg.agents?.defaults?.model ?? null,
  fallbackModels: cfg.agents?.defaults?.model?.fallbacks ?? cfg.agents?.defaults?.fallbackModels ?? [],
  agents,
};

const runtimeAgents = (Array.isArray(agentsList) ? agentsList : []).map((a) => ({
  id: a.id || a.name,
  model: a.model ?? null,
  channels: a.channels ?? [],
  online: a.online ?? true,
}));

const runtimeSnapshot = {
  status: { healthy: status?.healthy ?? true, activeAgents: runtimeAgents.length },
  agents: runtimeAgents,
  globalDefaultModel: status?.globalDefaultModel ?? null,
  fallbackModels: status?.fallbackModels ?? [],
};

const statusExtra = { openclawVersion: versionRaw };

// Build model profiles from CLI output, falling back to raw config
const modelsSource = (models && typeof models === "object")
  ? models
  : (cfg.agents?.defaults?.models || cfg.models || {});
const modelProfiles = Object.entries(modelsSource).map(([id, m]) => {
  // Model id format is "provider/model" — extract parts
  const parts = id.split("/");
  return {
    id,
    provider: m?.provider || parts[0] || "unknown",
    model: m?.model || parts.slice(1).join("/") || id,
    enabled: true,
  };
});

const channelsConfig = { channels: channels?.list ?? [], bindings: channels?.bindings ?? [] };
const cronConfig = { jobs: cron?.jobs ?? [] };

console.log(`Pre-fetch done in ${Date.now() - startMs}ms`);

// Fail hard if critical SSH commands returned no data
// Status requires a running gateway; skip if unavailable
if (!status) console.warn("openclaw status --json: no data (gateway not running?)");
const failed = [];
if (!agentsList && agents.length === 0) failed.push("openclaw agents list --json");
if (!models && modelProfiles.length === 0) failed.push("models (neither CLI nor config)");
if (failed.length > 0) {
  console.error("FATAL: No agent/model data available:");
  failed.forEach((c) => console.error("  -", c));
  process.exit(1);
}

// ---- Cached response map ----
const cache = {
  get_instance_config_snapshot: configSnapshot,
  get_instance_runtime_snapshot: runtimeSnapshot,
  get_status_extra: statusExtra,
  get_status_light: { healthy: status?.healthy ?? true, activeAgents: runtimeAgents.length },
  list_model_profiles: modelProfiles,
  list_agents_overview: runtimeAgents,
  get_channels_config_snapshot: channelsConfig,
  get_channels_runtime_snapshot: { channels: [], bindings: [], agents: [] },
  get_cron_config_snapshot: cronConfig,
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
  console.log(`IPC Bridge Server listening on http://localhost:${PORT} (cached mode)`);
});
