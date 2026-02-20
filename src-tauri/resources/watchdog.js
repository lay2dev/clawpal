#!/usr/bin/env node
'use strict';

// ---------------------------------------------------------------------------
// watchdog.js -- Zero-dependency Node.js watchdog for OpenClaw cron jobs.
//
// Runs as a long-lived background process on the OpenClaw host machine.
// It periodically checks whether scheduled cron jobs have actually executed
// and, when a missed execution is detected, attempts remediation by
// restarting the gateway (if necessary) and re-triggering the job.
//
// Usage:
//   node watchdog.js          # stdout logging; caller redirects to file
// ---------------------------------------------------------------------------

const fs   = require('fs');
const path = require('path');
const net  = require('net');
const { execSync, exec } = require('child_process');

// -- Paths & constants -------------------------------------------------------

const OPENCLAW_DIR  = path.join(process.env.HOME || '', '.openclaw');
const WATCHDOG_DIR  = path.join(OPENCLAW_DIR, 'watchdog');
const PID_FILE      = path.join(WATCHDOG_DIR, 'watchdog.pid');
const STATUS_FILE   = path.join(WATCHDOG_DIR, 'status.json');
const CONFIG_FILE   = path.join(OPENCLAW_DIR, 'openclaw.json');
const JOBS_FILE     = path.join(OPENCLAW_DIR, 'cron', 'jobs.json');
const RUNS_DIR      = path.join(OPENCLAW_DIR, 'cron', 'runs');

const CHECK_INTERVAL = 60_000;        // 60 s main-loop tick
const BACKOFF        = [30_000, 60_000, 120_000];
const MAX_RETRIES    = 3;
const TCP_TIMEOUT    = 2_000;         // 2 s for gateway probe
const EXEC_TIMEOUT   = 30_000;        // 30 s for CLI calls
const GATEWAY_WAIT   = 15_000;        // 15 s max wait after `openclaw up`

const startedAt = new Date().toISOString();

// -- Logging -----------------------------------------------------------------

function log(msg) {
  const ts = new Date().toISOString();
  process.stdout.write(`[${ts}] ${msg}\n`);
}

// -- Filesystem helpers ------------------------------------------------------

/** Ensure a directory exists (recursive). */
function ensureDir(dir) {
  try {
    fs.mkdirSync(dir, { recursive: true });
  } catch (_) { /* already exists or unrecoverable */ }
}

/** Read and JSON-parse a file. Returns `undefined` on any failure. */
function readJson(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (_) {
    return undefined;
  }
}

/** Atomically write JSON (write to tmp, then rename). */
function writeJson(filePath, data) {
  const tmp = filePath + '.tmp';
  try {
    fs.writeFileSync(tmp, JSON.stringify(data, null, 2) + '\n', 'utf8');
    fs.renameSync(tmp, filePath);
  } catch (err) {
    log(`WARN: failed to write ${filePath}: ${err.message}`);
  }
}

// -- PID-file management -----------------------------------------------------

/** Check whether a process with the given PID is alive. */
function isProcessAlive(pid) {
  try {
    process.kill(pid, 0); // signal 0 = existence check
    return true;
  } catch (_) {
    return false;
  }
}

/**
 * Ensure only one watchdog instance runs at a time.
 * Exits the process if another instance is already alive.
 */
function acquirePidFile() {
  ensureDir(WATCHDOG_DIR);

  try {
    const existing = fs.readFileSync(PID_FILE, 'utf8').trim();
    const pid = parseInt(existing, 10);
    if (pid && isProcessAlive(pid)) {
      log(`Another watchdog is already running (PID ${pid}). Exiting.`);
      process.exit(0);
    }
  } catch (_) {
    // PID file doesn't exist or unreadable -- fine, carry on.
  }

  fs.writeFileSync(PID_FILE, String(process.pid) + '\n', 'utf8');
  log(`Watchdog started (PID ${process.pid}).`);
}

/** Remove the PID file so a future instance can start. */
function releasePidFile() {
  try {
    const content = fs.readFileSync(PID_FILE, 'utf8').trim();
    if (parseInt(content, 10) === process.pid) {
      fs.unlinkSync(PID_FILE);
    }
  } catch (_) { /* best effort */ }
}

// -- Gateway helpers ---------------------------------------------------------

/** Read the gateway port from openclaw.json (default 18789). */
function getGatewayPort() {
  try {
    const cfg = readJson(CONFIG_FILE);
    if (cfg && cfg.gateway && typeof cfg.gateway.port === 'number') {
      return cfg.gateway.port;
    }
  } catch (_) { /* fall through */ }
  return 18789;
}

/**
 * TCP-probe a port on localhost.
 * Resolves `true` if the port is open, `false` otherwise.
 */
function probePort(port) {
  return new Promise((resolve) => {
    const sock = net.createConnection({ host: '127.0.0.1', port }, () => {
      sock.destroy();
      resolve(true);
    });
    sock.setTimeout(TCP_TIMEOUT);
    sock.on('timeout', () => { sock.destroy(); resolve(false); });
    sock.on('error',   () => { sock.destroy(); resolve(false); });
  });
}

/**
 * Attempt `openclaw up` and wait up to GATEWAY_WAIT ms for the port to open.
 * Returns `true` if the gateway becomes reachable.
 */
async function ensureGatewayUp(port) {
  try {
    log('Gateway port closed. Running `openclaw up`...');
    execSync('openclaw up', { timeout: EXEC_TIMEOUT, stdio: 'ignore' });
  } catch (err) {
    log(`WARN: \`openclaw up\` failed: ${err.message}`);
    return false;
  }

  // Poll every 1 s up to GATEWAY_WAIT
  const deadline = Date.now() + GATEWAY_WAIT;
  while (Date.now() < deadline) {
    if (await probePort(port)) {
      log('Gateway is now reachable.');
      return true;
    }
    await sleep(1000);
  }
  log('Gateway did not become reachable in time.');
  return false;
}

// -- Schedule parsing --------------------------------------------------------

/**
 * Expand a single cron field token into an array of valid integers.
 *
 * Supports: `*`, single numbers, ranges (`1-5`), steps (`* /15`, `1-30/5`),
 * and comma-separated lists (`1,3,5`).
 *
 * @param {string} token  - The raw field string.
 * @param {number} min    - Minimum valid value (inclusive).
 * @param {number} max    - Maximum valid value (inclusive).
 * @returns {number[]}    - Sorted list of matching values.
 */
function expandCronField(token, min, max) {
  const results = new Set();

  const parts = token.split(',');
  for (const part of parts) {
    let stepMatch = part.match(/^(.+)\/(\d+)$/);
    let step = stepMatch ? parseInt(stepMatch[2], 10) : 1;
    let range = stepMatch ? stepMatch[1] : part;

    let start, end;
    if (range === '*') {
      start = min;
      end   = max;
    } else if (range.includes('-')) {
      const [a, b] = range.split('-').map(Number);
      start = a;
      end   = b;
    } else {
      start = parseInt(range, 10);
      end   = start;
    }

    for (let i = start; i <= end; i += step) {
      if (i >= min && i <= max) results.add(i);
    }
  }

  return Array.from(results).sort((a, b) => a - b);
}

/**
 * Parse a 5-field cron expression and find the most recent Date that matches,
 * walking backward from `now`.
 *
 * Fields: minute  hour  day-of-month  month  day-of-week
 *         (0-59)  (0-23) (1-31)       (1-12) (0-6, 0=Sun)
 *
 * We walk back day-by-day (up to 366 days) then within matching days find the
 * latest matching hour:minute.
 */
function lastCronTrigger(expr, now) {
  const fields = expr.trim().split(/\s+/);
  if (fields.length !== 5) return null;

  const minutes  = expandCronField(fields[0], 0, 59);
  const hours    = expandCronField(fields[1], 0, 23);
  const mdays    = expandCronField(fields[2], 1, 31);
  const months   = expandCronField(fields[3], 1, 12);
  const wdays    = expandCronField(fields[4], 0, 6);

  // Walk back up to 366 days from now
  for (let d = 0; d <= 366; d++) {
    const candidate = new Date(now.getTime() - d * 86400_000);
    const cMonth = candidate.getMonth() + 1;  // 1-based
    const cMday  = candidate.getDate();
    const cWday  = candidate.getDay();         // 0=Sun

    if (!months.includes(cMonth)) continue;
    if (!mdays.includes(cMday))   continue;
    if (!wdays.includes(cWday))   continue;

    // Within this matching day find the latest matching time <= now
    for (let hi = hours.length - 1; hi >= 0; hi--) {
      for (let mi = minutes.length - 1; mi >= 0; mi--) {
        const t = new Date(candidate);
        t.setHours(hours[hi], minutes[mi], 0, 0);
        if (t <= now) return t;
      }
    }
  }

  return null;
}

/**
 * Compute the last expected trigger time for an interval-based schedule.
 * We align to the Unix epoch so every watchdog instance agrees.
 */
function lastIntervalTrigger(everyMs, now) {
  if (!everyMs || everyMs <= 0) return null;
  const elapsed = now.getTime() % everyMs;
  return new Date(now.getTime() - elapsed);
}

/**
 * Compute the last expected trigger time for a one-shot "at" schedule.
 * Simply returns the target Date if it is in the past.
 */
function lastAtTrigger(isoString, now) {
  try {
    const t = new Date(isoString);
    return t <= now ? t : null;
  } catch (_) {
    return null;
  }
}

/**
 * Determine the last expected trigger time for a job based on its schedule.
 */
function lastExpectedTrigger(schedule, now) {
  if (!schedule || !schedule.kind) return null;

  switch (schedule.kind) {
    case 'cron':
      return schedule.expression
        ? lastCronTrigger(schedule.expression, now)
        : null;
    case 'every':
      return typeof schedule.everyMs === 'number'
        ? lastIntervalTrigger(schedule.everyMs, now)
        : null;
    case 'at':
      return schedule.at
        ? lastAtTrigger(schedule.at, now)
        : null;
    default:
      return null;
  }
}

// -- Run-record helpers ------------------------------------------------------

/**
 * Read the last line from a .jsonl file and parse the `ts` field.
 * Returns a Date or null.
 */
function lastRunTime(jobId) {
  const file = path.join(RUNS_DIR, `${jobId}.jsonl`);
  try {
    const data = fs.readFileSync(file, 'utf8').trim();
    if (!data) return null;
    const lines = data.split('\n');
    const last  = JSON.parse(lines[lines.length - 1]);
    if (last.ts) return new Date(last.ts);
    if (last.startedAt) return new Date(last.startedAt);
    return null;
  } catch (_) {
    return null;
  }
}

// -- Job normalisation -------------------------------------------------------

/**
 * Normalise jobs.json content into an array of `{ jobId, ...rest }`.
 * Handles both object-keyed and array formats.
 */
function normaliseJobs(raw) {
  if (!raw) return [];

  if (Array.isArray(raw)) {
    return raw.filter((j) => j && j.jobId);
  }

  if (typeof raw === 'object') {
    return Object.entries(raw).map(([id, job]) => ({
      jobId: id,
      ...job,
    }));
  }

  return [];
}

// -- Remediation -------------------------------------------------------------

/**
 * Trigger a single job via the CLI.
 * Returns `true` on success, throws on failure.
 */
function triggerJob(jobId) {
  log(`Triggering job "${jobId}" via \`openclaw cron run ${jobId} --due\`...`);
  execSync(`openclaw cron run ${jobId} --due`, {
    timeout: EXEC_TIMEOUT,
    stdio: 'ignore',
  });
  log(`Job "${jobId}" triggered successfully.`);
}

// -- Utility -----------------------------------------------------------------

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// -- Per-job state (in-memory between cycles) --------------------------------

/** { [jobId]: { status, retries, lastError, escalatedAt, scheduleKey } } */
const jobState = {};

function getJobState(jobId) {
  if (!jobState[jobId]) {
    jobState[jobId] = { status: 'ok', retries: 0, lastError: null, escalatedAt: null };
  }
  return jobState[jobId];
}

// -- Main check cycle --------------------------------------------------------

async function runCheckCycle() {
  const now  = new Date();
  const port = getGatewayPort();

  log('--- Check cycle start ---');

  const rawJobs = readJson(JOBS_FILE);
  const jobs    = normaliseJobs(rawJobs);

  if (jobs.length === 0) {
    log('No jobs found in jobs.json (or file missing). Skipping.');
    writeStatus(now, null, {});
    return;
  }

  // Pre-probe gateway once per cycle (lazy; will probe on demand if needed)
  let gatewayAlive = null; // null = not yet tested

  const jobStatuses = {};

  for (const job of jobs) {
    const id = job.jobId;
    if (job.enabled === false) {
      // Explicitly disabled -- skip silently
      continue;
    }

    const state = getJobState(id);

    try {
      const schedule       = job.schedule;
      const expectedTrigger = lastExpectedTrigger(schedule, now);
      const lastRun         = lastRunTime(id);

      // Determine schedule key so we can reset escalation on new cycle
      const scheduleKey = expectedTrigger ? expectedTrigger.toISOString() : null;

      // Reset escalation when a new schedule cycle begins
      if (state.escalatedAt && scheduleKey && scheduleKey !== state._scheduleKey) {
        state.status      = 'ok';
        state.retries     = 0;
        state.lastError   = null;
        state.escalatedAt = null;
      }
      state._scheduleKey = scheduleKey;

      const missed =
        expectedTrigger &&
        (!lastRun || expectedTrigger.getTime() > lastRun.getTime());

      if (!missed) {
        // Job is on track
        if (state.status !== 'escalated') {
          state.status  = 'ok';
          state.retries = 0;
          state.lastError = null;
        }
        jobStatuses[id] = buildJobStatus(state, expectedTrigger, lastRun);
        continue;
      }

      // -- Missed job detected -----------------------------------------------

      // If already escalated for this schedule cycle, skip
      if (state.status === 'escalated') {
        log(`Job "${id}" still escalated. Skipping until next schedule cycle.`);
        jobStatuses[id] = buildJobStatus(state, expectedTrigger, lastRun);
        continue;
      }

      // If retries exhausted, escalate
      if (state.retries >= MAX_RETRIES) {
        log(`Job "${id}" exceeded max retries (${MAX_RETRIES}). Escalating.`);
        state.status      = 'escalated';
        state.escalatedAt = now.toISOString();
        jobStatuses[id]   = buildJobStatus(state, expectedTrigger, lastRun);
        continue;
      }

      // Apply backoff if retrying
      if (state.retries > 0) {
        const backoffMs = BACKOFF[Math.min(state.retries - 1, BACKOFF.length - 1)];
        log(`Job "${id}" retry #${state.retries}, backing off ${backoffMs / 1000}s...`);
        state.status = 'retrying';
        await sleep(backoffMs);
      } else {
        state.status = 'pending';
      }

      // Probe gateway (once per cycle, cached)
      if (gatewayAlive === null) {
        gatewayAlive = await probePort(port);
        log(`Gateway probe on port ${port}: ${gatewayAlive ? 'open' : 'closed'}`);
      }

      // Ensure gateway is up
      if (!gatewayAlive) {
        gatewayAlive = await ensureGatewayUp(port);
        if (!gatewayAlive) {
          state.retries++;
          state.lastError = 'Gateway could not be started';
          state.status    = 'retrying';
          jobStatuses[id] = buildJobStatus(state, expectedTrigger, lastRun);
          continue;
        }
      }

      // Trigger the job
      state.status = 'triggered';
      try {
        triggerJob(id);
        state.retries   = 0;
        state.lastError = null;
      } catch (err) {
        state.retries++;
        state.lastError = err.message;
        state.status    = 'retrying';
        log(`WARN: trigger for "${id}" failed: ${err.message}`);
      }

      jobStatuses[id] = buildJobStatus(state, expectedTrigger, lastRun);

    } catch (err) {
      log(`ERROR processing job "${id}": ${err.message}`);
      state.lastError = err.message;
      jobStatuses[id] = buildJobStatus(state, null, null);
    }
  }

  writeStatus(now, gatewayAlive, jobStatuses);
  log('--- Check cycle end ---');
}

function buildJobStatus(state, expectedTrigger, lastRun) {
  return {
    status:          state.status,
    lastScheduledAt: expectedTrigger ? expectedTrigger.toISOString() : null,
    lastRunAt:       lastRun ? lastRun.toISOString() : null,
    retries:         state.retries,
    lastError:       state.lastError || null,
    escalatedAt:     state.escalatedAt || null,
  };
}

function writeStatus(checkTime, gatewayAlive, jobStatuses) {
  ensureDir(WATCHDOG_DIR);
  const payload = {
    pid:              process.pid,
    startedAt:        startedAt,
    lastCheckAt:      checkTime.toISOString(),
    gatewayHealthy:   gatewayAlive === true,
    jobs:             jobStatuses,
  };
  writeJson(STATUS_FILE, payload);
}

// -- Entrypoint --------------------------------------------------------------

async function main() {
  acquirePidFile();

  // Schedule recurring checks
  async function loop() {
    while (true) {
      try {
        await runCheckCycle();
      } catch (err) {
        log(`FATAL in check cycle (recovered): ${err.message}`);
      }
      await sleep(CHECK_INTERVAL);
    }
  }

  loop();
}

// -- Graceful shutdown -------------------------------------------------------

function shutdown(signal) {
  log(`Received ${signal}. Cleaning up and exiting.`);
  releasePidFile();
  process.exit(0);
}

process.on('SIGTERM', () => shutdown('SIGTERM'));
process.on('SIGINT',  () => shutdown('SIGINT'));
process.on('exit',    () => {
  // Belt-and-suspenders: ensure PID file removed even on unexpected exit
  releasePidFile();
});

// -- Start -------------------------------------------------------------------

main().catch((err) => {
  log(`FATAL: ${err.message}`);
  releasePidFile();
  process.exit(1);
});
