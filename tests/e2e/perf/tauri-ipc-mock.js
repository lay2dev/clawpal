/**
 * Tauri IPC mock — injected via addInitScript before the app loads.
 * Uses the same pattern as @tauri-apps/api/mocks but inline (no import needed).
 */
(function () {
  const FIXTURES = window.__PERF_FIXTURES__ || {};
  const LATENCY_MS = parseInt(window.__PERF_MOCK_LATENCY__ || "50", 10);

  let _runtimeSnapshotCallCount = 0;
  const _COLD_START_SKIP = parseInt(window.__PERF_COLD_START_SKIP__ || "0", 10);

  const handlers = {
    get_instance_config_snapshot: () => {
      if (_COLD_START_SKIP > 0 && _runtimeSnapshotCallCount <= _COLD_START_SKIP) return null;
      return FIXTURES.configSnapshot;
    },
    get_instance_runtime_snapshot: () => {
      _runtimeSnapshotCallCount++;
      if (_COLD_START_SKIP > 0 && _runtimeSnapshotCallCount <= _COLD_START_SKIP) return null;
      return FIXTURES.runtimeSnapshot;
    },
    get_status_extra: () => FIXTURES.statusExtra,
    list_model_profiles: () => FIXTURES.modelProfiles || [],
    get_status_light: () => FIXTURES.runtimeSnapshot?.status || { healthy: true, activeAgents: 2 },
    queued_commands_count: () => 0,
    check_openclaw_update: () => ({ upgradeAvailable: false, latestVersion: null, installedVersion: FIXTURES.statusExtra?.openclawVersion }),
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
    read_raw_config: () => JSON.stringify({}),
    get_system_status: () => ({ platform: "linux", arch: "x64" }),
    list_channels_minimal: () => [],
    list_bindings: () => [],
    list_discord_guild_channels: () => [],
    get_channels_config_snapshot: () => ({ channels: [], bindings: [] }),
    get_channels_runtime_snapshot: () => ({ channels: [], bindings: [], agents: [] }),
    get_cron_config_snapshot: () => ({ jobs: [] }),
    get_cron_runtime_snapshot: () => ({ jobs: [], watchdog: null }),
    get_watchdog_status: () => ({ alive: false, deployed: false }),
    list_cron_jobs: () => [],
    list_history: () => ({ items: [] }),
    list_session_files: () => [],
    list_backups: () => [],
    get_rescue_bot_status: () => ({ action: "status", profile: "rescue", mainPort: 18789, rescuePort: 19789, minRecommendedPort: 19789, configured: false, active: false, runtimeState: "unconfigured", wasAlreadyConfigured: false, commands: [] }),
    migrate_legacy_instances: () => null,
    list_registered_instances: () => [{ id: "local", instanceType: "local", label: "Local", createdAt: Date.now() }],
    discover_local_instances: () => [],
    list_ssh_hosts: () => [],
    list_ssh_config_hosts: () => [],
    set_active_openclaw_home: () => null,
    set_active_clawpal_data_dir: () => null,
    precheck_registry: () => ({ ok: true }),
    precheck_transport: () => ({ ok: true }),
    precheck_instance: () => ({ ok: true }),
    precheck_auth: () => ({ ok: true }),
    connect_local_instance: () => null,
    ssh_status: () => ({ connected: false }),
    list_agents_overview: () => FIXTURES.runtimeSnapshot?.agents || [],
    record_install_experience: () => null,
    "plugin:event|listen": () => 0,
    "plugin:event|unlisten": () => null,
  };

  // Set up __TAURI_INTERNALS__ before any module loads
  window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = window.__TAURI_EVENT_PLUGIN_INTERNALS__ || {};

  const callbacks = new Map();
  let nextId = 1;

  window.__TAURI_INTERNALS__.invoke = async function (cmd, args) {
    await new Promise((r) => setTimeout(r, LATENCY_MS));
    if (handlers[cmd]) {
      return handlers[cmd](args);
    }
    // Silently return null for unhandled commands to avoid errors
    return null;
  };

  window.__TAURI_INTERNALS__.transformCallback = function (callback, once) {
    const id = nextId++;
    callbacks.set(id, { callback, once });
    return id;
  };

  window.__TAURI_INTERNALS__.unregisterCallback = function (id) {
    callbacks.delete(id);
  };

  window.__TAURI_INTERNALS__.runCallback = function (id, data) {
    const entry = callbacks.get(id);
    if (entry) {
      if (entry.once) callbacks.delete(id);
      entry.callback(data);
    }
  };

  window.__TAURI_INTERNALS__.callbacks = callbacks;

  window.__TAURI_INTERNALS__.convertFileSrc = function (path) {
    return path;
  };

  window.__TAURI_INTERNALS__.metadata = {
    currentWindow: { label: "main" },
    currentWebview: { windowLabel: "main", label: "main" },
  };

  window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener = function () {};
})();
