/**
 * Home page render performance E2E test.
 *
 * Opens the app in Vite dev server with Tauri IPC mock, clicks into the local
 * instance, and collects render probe timings from window.__RENDER_PROBES__.
 */
import { test, expect } from "@playwright/test";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const FIXTURES_DIR = join(__dirname, "fixtures");
const REPORT_PATH = join(__dirname, "report.md");
const MOCK_SCRIPT = readFileSync(join(__dirname, "tauri-ipc-mock.js"), "utf-8");
const RUNS = 3;
const SETTLED_GATE_MS = parseInt(process.env.PERF_SETTLED_GATE_MS || "5000", 10);
const MOCK_LATENCY_MS = process.env.PERF_MOCK_LATENCY_MS || "50";

function loadFixture(name) {
  const p = join(FIXTURES_DIR, `${name}.json`);
  if (!existsSync(p)) return null;
  return JSON.parse(readFileSync(p, "utf-8"));
}

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
  md += `**Run** #${run} · \`${commit}\` · ${date} · mock latency ${MOCK_LATENCY_MS}ms\n\n`;
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

test("home page render timing", async ({ page }) => {
  const fixtures = {
    configSnapshot: loadFixture("configSnapshot"),
    runtimeSnapshot: loadFixture("runtimeSnapshot"),
    statusExtra: loadFixture("statusExtra"),
    modelProfiles: loadFixture("modelProfiles"),
  };

  await page.addInitScript({
    content: `
      window.__PERF_FIXTURES__ = ${JSON.stringify(fixtures)};
      window.__PERF_MOCK_LATENCY__ = "${MOCK_LATENCY_MS}";
      ${MOCK_SCRIPT}
    `,
  });

  const allRuns = [];

  for (let i = 0; i < RUNS; i++) {
    await page.goto("http://localhost:1420");

    // Wait for app to render the Start page, then click the local instance card
    // to navigate into Home
    await page.waitForTimeout(2000); // Let app initialize

    // Clear IPC read cache so Home probes measure real IPC fetch + render,
    // not cache-hit render time (which would be ~8ms instead of ~60ms).
    await page.evaluate(() => {
      if (window.__TEST_CLEAR_READ_CACHE__) window.__TEST_CLEAR_READ_CACHE__();
      delete window.__RENDER_PROBES__;
    });

    // Click the local instance card — look for it by text or role
    const instanceCard = page.locator('text=local').first();
    if (await instanceCard.isVisible({ timeout: 5000 }).catch(() => false)) {
      await instanceCard.click();
    }

    // Wait for settled probe
    try {
      await page.waitForFunction(
        () => window.__RENDER_PROBES__?.home?.settled != null,
        { timeout: 20_000 },
      );
    } catch {
      // If probes didn't fire, try to collect partial data
      console.warn(`Run ${i}: settled probe did not fire within timeout`);
    }

    const probes = await page.evaluate(() => window.__RENDER_PROBES__?.home || {});
    if (Object.keys(probes).length > 0) {
      allRuns.push(probes);
    }
  }

  // Need at least 1 successful run
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
