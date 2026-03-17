/**
 * Tauri IPC mock for Vite dev server.
 * Injected into the page before React loads.
 * Routes invoke() calls to fixture JSON data.
 */
(function () {
  const FIXTURES = window.__PERF_FIXTURES__ || {};
  const LATENCY_MS = parseInt(window.__PERF_MOCK_LATENCY__ || "50", 10);

  function delay(ms) {
    return new Promise((r) => setTimeout(r, ms));
  }

  const handlers = {
    get_instance_config_snapshot: () => FIXTURES.configSnapshot,
    get_instance_runtime_snapshot: () => FIXTURES.runtimeSnapshot,
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
  };

  window.__TAURI_INTERNALS__ = {
    invoke: async function (cmd, args) {
      await delay(LATENCY_MS);
      if (handlers[cmd]) {
        return handlers[cmd](args);
      }
      console.warn(`[ipc-mock] unhandled command: ${cmd}`);
      return null;
    },
    metadata: {
      currentWebview: { label: "main" },
      currentWindow: { label: "main" },
    },
    convertFileSrc: (path) => path,
  };
})();
