#!/usr/bin/env node
/**
 * IPC Bridge Server — proxies Tauri invoke() calls to a real OpenClaw
 * instance running in Docker via SSH.
 *
 * Starts an HTTP server on BRIDGE_PORT (default 3399). The browser-side
 * tauri-ipc-bridge.js sends POST {cmd, args} and receives the real response.
 */
import { createServer } from "node:http";
import { execSync } from "node:child_process";

const PORT = parseInt(process.env.BRIDGE_PORT || "3399", 10);
const SSH_PORT = process.env.CLAWPAL_PERF_SSH_PORT || "2299";
const SSH_PREFIX = `sshpass -p clawpal-perf-e2e ssh -o StrictHostKeyChecking=no -p ${SSH_PORT} root@localhost`;

/** Run a command on the Docker container via SSH, return stdout. */
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

/** Parse JSON from openclaw CLI output, stripping any leading non-JSON lines. */
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

/**
 * Map Tauri command names to SSH-based implementations.
 * Each handler returns a JSON-serialisable value (or null).
 */
const handlers = {
  get_instance_config_snapshot() {
    const raw = ssh("openclaw config get --json");
    const cfg = parseCliJson(raw);
    if (!cfg) return { globalDefaultModel: null, fallbackModels: [], agents: [] };
    const agents = (cfg.agents?.list ?? []).map((a) => ({
      id: a.id,
      model: a.model ?? null,
      channels: [],
      online: false,
    }));
    return {
      globalDefaultModel: cfg.agents?.defaults?.model ?? cfg.defaults?.model ?? null,
      fallbackModels: cfg.agents?.defaults?.fallbackModels ?? cfg.defaults?.fallbackModels ?? [],
      agents,
    };
  },

  get_instance_runtime_snapshot() {
    const statusRaw = ssh("openclaw status --json");
    const agentsRaw = ssh("openclaw agents list --json");
    const status = parseCliJson(statusRaw);
    const agentsList = parseCliJson(agentsRaw);

    const agents = (Array.isArray(agentsList) ? agentsList : []).map((a) => ({
      id: a.id || a.name,
      model: a.model ?? null,
      channels: a.channels ?? [],
      online: a.online ?? true,
    }));

    return {
      status: {
        healthy: status?.healthy ?? true,
        activeAgents: agents.length,
      },
      agents,
      globalDefaultModel: status?.globalDefaultModel ?? null,
      fallbackModels: status?.fallbackModels ?? [],
    };
  },

  get_status_extra() {
    const ver = ssh("openclaw --version 2>/dev/null") || "unknown";
    return { openclawVersion: ver };
  },

  list_model_profiles() {
    const raw = ssh("openclaw config get models --json");
    const models = parseCliJson(raw);
    if (!models || typeof models !== "object") return [];
    return Object.entries(models).map(([id, m]) => ({
      id,
      provider: m.provider,
      model: m.model,
      enabled: true,
    }));
  },

  get_status_light() {
    const raw = ssh("openclaw status --json");
    const status = parseCliJson(raw);
    return { healthy: status?.healthy ?? true, activeAgents: status?.activeAgents ?? 0 };
  },

  list_agents_overview() {
    const raw = ssh("openclaw agents list --json");
    const agents = parseCliJson(raw);
    return Array.isArray(agents) ? agents : [];
  },

  get_channels_config_snapshot() {
    const raw = ssh("openclaw config get channels --json");
    const cfg = parseCliJson(raw);
    return { channels: cfg?.list ?? [], bindings: cfg?.bindings ?? [] };
  },
  get_channels_runtime_snapshot: () => ({ channels: [], bindings: [], agents: [] }),

  get_cron_config_snapshot() {
    const raw = ssh("openclaw config get cron --json");
    const cfg = parseCliJson(raw);
    return { jobs: cfg?.jobs ?? [] };
  },
  get_cron_runtime_snapshot: () => ({ jobs: [], watchdog: null }),

  get_rescue_bot_status: () => ({
    action: "status", profile: "rescue", mainPort: 18789, rescuePort: 19789,
    minRecommendedPort: 19789, configured: false, active: false,
    runtimeState: "unconfigured", wasAlreadyConfigured: false, commands: [],
  }),

  // Lightweight / no-op commands
  queued_commands_count: () => 0,
  check_openclaw_update: () => ({ upgradeAvailable: false, latestVersion: null }),
  log_app_event: () => true,
  get_app_preferences: () => ({}),
  get_bug_report_settings: () => ({}),
  get_bug_report_stats: () => ({}),
  ensure_access_profile: () => ({}),
  get_cached_model_catalog: () => [],
  list_recipes: () => [],
  install_list_methods: () => [],
  list_ssh_hosts: () => [],
  local_openclaw_config_exists: () => true,
  local_openclaw_cli_available: () => true,
  read_raw_config() {
    const raw = ssh("cat /root/.openclaw/openclaw.json");
    return raw || "{}";
  },
  get_system_status: () => ({ platform: "linux", arch: "x64" }),
  list_channels_minimal: () => [],
  list_bindings: () => [],
  list_discord_guild_channels: () => [],
  get_watchdog_status: () => ({ alive: false, deployed: false }),
  list_cron_jobs: () => [],
  list_history: () => ({ items: [] }),
  list_session_files: () => [],
  list_backups: () => [],
  migrate_legacy_instances: () => null,
  list_registered_instances: () => [{ id: "local", instanceType: "local", label: "Local", createdAt: Date.now() }],
  discover_local_instances: () => [],
  list_ssh_config_hosts: () => [],
  set_active_openclaw_home: () => null,
  set_active_clawpal_data_dir: () => null,
  precheck_registry: () => ({ ok: true }),
  precheck_transport: () => ({ ok: true }),
  precheck_instance: () => ({ ok: true }),
  precheck_auth: () => ({ ok: true }),
  connect_local_instance: () => null,
  ssh_status: () => ({ connected: false }),
  record_install_experience: () => null,
};

const server = createServer(async (req, res) => {
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Methods", "POST, OPTIONS");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type");

  if (req.method === "OPTIONS") {
    res.writeHead(204);
    return res.end();
  }

  if (req.method !== "POST" || req.url !== "/invoke") {
    res.writeHead(404);
    return res.end("Not found");
  }

  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const body = JSON.parse(Buffer.concat(chunks).toString());

  const { cmd, args } = body;
  const handler = handlers[cmd];

  if (!handler) {
    res.writeHead(200, { "Content-Type": "application/json" });
    return res.end(JSON.stringify({ ok: true, result: null }));
  }

  try {
    const result = typeof handler === "function" ? handler(args) : handler;
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ ok: true, result }));
  } catch (e) {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ ok: false, error: e.message }));
  }
});

server.listen(PORT, () => {
  console.log(`IPC Bridge Server listening on http://localhost:${PORT}`);
});
