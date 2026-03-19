/**
 * Tauri IPC Bridge — replaces the static mock with a live HTTP proxy to
 * the IPC bridge server, which in turn calls real OpenClaw CLI via SSH.
 *
 * Injected via page.addInitScript() before the app loads.
 * Measures real IPC round-trip latency instead of mock delays.
 */
(function () {
  const BRIDGE_URL = window.__PERF_BRIDGE_URL__ || "http://localhost:3399";

  window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = window.__TAURI_EVENT_PLUGIN_INTERNALS__ || {};

  const callbacks = new Map();
  let nextId = 1;

  window.__TAURI_INTERNALS__.invoke = async function (cmd, args) {
    // Event plugin commands are handled locally (no backend equivalent)
    if (cmd === "plugin:event|listen") return 0;
    if (cmd === "plugin:event|unlisten") return null;

    try {
      const resp = await fetch(`${BRIDGE_URL}/invoke`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ cmd, args }),
      });
      const data = await resp.json();
      if (data.ok) return data.result;
      throw new Error(data.error || "Bridge error");
    } catch (e) {
      // Silently return null for failed bridge calls (same as mock fallback)
      console.warn(`[ipc-bridge] ${cmd} failed:`, e.message);
      return null;
    }
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
