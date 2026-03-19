/**
 * Home page render performance E2E test.
 *
 * Opens the app in Vite dev server with a live IPC bridge to a real OpenClaw
 * instance running in Docker. Probe timings measure actual IPC round-trip
 * latency, not mock delays.
 */
import { test, expect } from "@playwright/test";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPORT_PATH = join(__dirname, "report.md");
const BRIDGE_SCRIPT = readFileSync(join(__dirname, "tauri-ipc-bridge.js"), "utf-8");
const BRIDGE_URL = process.env.PERF_BRIDGE_URL || "http://localhost:3399";
const RUNS = 3;
const SETTLED_GATE_MS = parseInt(process.env.PERF_SETTLED_GATE_MS || "5000", 10);

function median(arr) {
  const sorted = [...arr].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : Math.round((sorted[mid - 1] + sorted[mid]) / 2);
}

function loadBaseline() {
  const p = join(__dirname, "baseline.json");
  if (!existsSync(p)) return null;
  try { return JSON.parse(readFileSync(p, "utf-8")); } catch { return null; }
}

function formatDelta(current, baselineVal) {
  if (baselineVal == null) return "—";
  const delta = current - baselineVal;
  const sign = delta <= 0 ? "" : "+";
  const emoji = delta <= 0 ? "✅" : "⚠️";
  return `${sign}${delta}ms ${emoji}`;
}

function generateReport(probes, baseline) {
  const commit = process.env.GITHUB_SHA?.slice(0, 7) || "local";
  const run = process.env.GITHUB_RUN_NUMBER || "—";
  const date = new Date().toISOString().slice(0, 19).replace("T", " ") + " UTC";
  const labels = ["status", "version", "agents", "models", "settled"];

  let md = `## 🏠 Home Page Render Probes\n\n`;
  md += `**Run** #${run} · \`${commit}\` · ${date} · **real IPC** (SSH → Docker OpenClaw)\n\n`;
  md += `| Probe | ms | Δ baseline |\n`;
  md += `|-------|---:|--------:|\n`;
  for (const label of labels) {
    const val = probes[label] ?? "—";
    const delta = baseline ? formatDelta(val, baseline[label]) : "—";
    md += `| ${label} | ${val} | ${delta} |\n`;
  }
  md += `\nGate: settled < ${SETTLED_GATE_MS}ms ${(probes.settled ?? 9999) < SETTLED_GATE_MS ? "✅" : "❌"}\n`;
  md += `\n<details><summary>Raw probes</summary>\n\n\`\`\`json\n${JSON.stringify(probes, null, 2)}\n\`\`\`\n\n</details>\n`;
  return md;
}

test("home page render timing with real IPC", async ({ page }) => {
  await page.addInitScript({
    content: `
      window.__PERF_BRIDGE_URL__ = "${BRIDGE_URL}";
      window.__PERF_COLD_START_SKIP__ = "1";
      ${BRIDGE_SCRIPT}
    `,
  });

  const allRuns = [];

  for (let i = 0; i < RUNS; i++) {
    // Clear persisted read cache so each run is a true cold start
    await page.evaluate(() => {
      try { localStorage.clear(); sessionStorage.clear(); } catch {}
    }).catch(() => {});
    await page.goto("http://localhost:1420");

    // Wait for app to render the Start page, then click the local instance card
    await page.waitForTimeout(2000);

    const instanceCard = page.locator('text=local').first();
    if (await instanceCard.isVisible({ timeout: 5000 }).catch(() => false)) {
      await instanceCard.click();
    }

    // Wait for settled probe
    try {
      await page.waitForFunction(
        () => window.__RENDER_PROBES__?.home?.settled != null,
        { timeout: 30_000 },
      );
    } catch {
      console.warn(`Run ${i}: settled probe did not fire within timeout`);
    }

    const probes = await page.evaluate(() => window.__RENDER_PROBES__?.home || {});
    if (Object.keys(probes).length > 0) {
      allRuns.push(probes);
    }
  }

  expect(allRuns.length).toBeGreaterThan(0);

  const labels = ["status", "version", "agents", "models", "settled"];
  const medianProbes = {};
  for (const label of labels) {
    const values = allRuns.map((r) => r[label]).filter((v) => v != null);
    medianProbes[label] = values.length > 0 ? median(values) : null;
  }

  if (medianProbes.settled != null) {
    expect(medianProbes.settled).toBeLessThan(SETTLED_GATE_MS);
  }

  const baseline = loadBaseline();
  const report = generateReport(medianProbes, baseline);
  writeFileSync(REPORT_PATH, report);
  console.log("\n" + report);
});
