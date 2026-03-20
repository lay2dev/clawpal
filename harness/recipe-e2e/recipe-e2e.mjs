import fs from "fs";
import path from "path";
import { execFileSync } from "child_process";
import { performance } from "perf_hooks";
import { Builder, By, Capabilities, Key } from "selenium-webdriver";

const SCREENSHOT_DIR = process.env.SCREENSHOT_DIR || "/screenshots";
const REPORT_DIR = process.env.REPORT_DIR || "/report";
const APP_BINARY = process.env.APP_BINARY || "/usr/local/bin/clawpal";
const SSH_HOST = process.env.OPENCLAW_SSH_HOST || "127.0.0.1";
const SSH_PORT = parseInt(process.env.OPENCLAW_SSH_PORT || "2222", 10);
const SSH_USER = process.env.OPENCLAW_SSH_USER || "root";
const SSH_PASSWORD = process.env.OPENCLAW_SSH_PASSWORD || "clawpal-recipe-e2e";
const REMOTE_IDENTITY_MAIN = "~/.openclaw/agents/main/agent/IDENTITY.md";
const REMOTE_CONFIG = "~/.openclaw/openclaw.json";
const BOOT_WAIT_MS = parseInt(process.env.BOOT_WAIT_MS || "6000", 10);
const STEP_WAIT_MS = parseInt(process.env.STEP_WAIT_MS || "800", 10);
const LONG_WAIT_MS = parseInt(process.env.LONG_WAIT_MS || "1500", 10);

const CHANNEL_SUPPORT_PERSONA = [
  "You are the support concierge for this channel.",
  "Welcome users, ask clarifying questions, and turn vague requests into clean next steps.",
].join("\n\n");

const AGENT_COACH_PERSONA = [
  "You are a focused coaching agent.",
  "Help the team make progress with short, direct guidance. Push for clarity, prioritization, and next actions.",
].join("\n\n");

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function roundMs(value) {
  return Math.round(value);
}

function xpathLiteral(value) {
  if (!value.includes("'")) {
    return `'${value}'`;
  }
  if (!value.includes('"')) {
    return `"${value}"`;
  }
  return `concat('${value.split("'").join(`',"'",'`)}')`;
}

async function sleep(driver, ms) {
  await driver.sleep(ms);
}

async function shot(driver, category, name) {
  const dir = path.join(SCREENSHOT_DIR, category);
  ensureDir(dir);
  const png = await driver.takeScreenshot();
  fs.writeFileSync(path.join(dir, `${name}.png`), Buffer.from(png, "base64"));
  console.log(`  screenshot: ${category}/${name}.png`);
}

async function pageText(driver) {
  try {
    return await driver.executeScript("return document.body ? document.body.innerText : '';");
  } catch {
    return "";
  }
}

async function waitForApp(driver) {
  console.log("Waiting for app boot");
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    try {
      const roots = await driver.findElements(By.css("#root > *"));
      if (roots.length > 0) {
        await sleep(driver, BOOT_WAIT_MS);
        return;
      }
    } catch {
      // Retry during boot transitions.
    }
    await sleep(driver, 1000);
  }
  throw new Error("Timed out waiting for React root to mount");
}

async function waitForText(driver, text, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const body = await pageText(driver);
    if (body.includes(text)) {
      return;
    }
    await sleep(driver, 500);
  }
  throw new Error(`Timed out waiting for text: ${text}`);
}

async function waitForAnyText(driver, texts, timeoutMs = 60_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const body = await pageText(driver);
    for (const text of texts) {
      if (body.includes(text)) {
        return text;
      }
    }
    await sleep(driver, 750);
  }
  throw new Error(`Timed out waiting for any of: ${texts.join(", ")}`);
}

async function waitForDisplayed(driver, locator, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const elements = await driver.findElements(locator);
      for (const element of elements) {
        if (await element.isDisplayed()) {
          return element;
        }
      }
    } catch {
      // Ignore transient stale frame errors.
    }
    await sleep(driver, 400);
  }
  throw new Error(`Timed out waiting for locator: ${locator}`);
}

async function clickElement(driver, element) {
  try {
    await driver.executeScript(
      "arguments[0].scrollIntoView({ block: 'center', inline: 'nearest' });",
      element,
    );
  } catch {
    // Best effort only.
  }

  try {
    await element.click();
  } catch {
    await driver.executeScript("arguments[0].click();", element);
  }

  await sleep(driver, STEP_WAIT_MS);
}

async function clearAndType(driver, element, value) {
  await clickElement(driver, element);
  await element.sendKeys(Key.chord(Key.CONTROL, "a"), Key.BACK_SPACE);
  if (value.length > 0) {
    await element.sendKeys(value);
  }
  await sleep(driver, 250);
}

async function fillById(driver, id, value) {
  const element = await waitForDisplayed(driver, By.css(`#${id}`));
  await clearAndType(driver, element, value);
}

async function clickNav(driver, label) {
  const button = await waitForDisplayed(
    driver,
    By.xpath(`//aside//button[.//*[normalize-space()=${xpathLiteral(label)}] or normalize-space()=${xpathLiteral(label)}]`),
    20_000,
  );
  await clickElement(driver, button);
}

async function clickButtonText(driver, labels, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const label of labels) {
      try {
        const button = await waitForDisplayed(
          driver,
          By.xpath(`//button[normalize-space()=${xpathLiteral(label)}]`),
          2000,
        );
        await clickElement(driver, button);
        return label;
      } catch {
        // Try next label or loop retry.
      }
    }
    await sleep(driver, 400);
  }
  throw new Error(`Timed out waiting for button: ${labels.join(", ")}`);
}

async function selectByTriggerId(driver, id, labels) {
  const trigger = await waitForDisplayed(driver, By.css(`#${id}`), 20_000);
  await clickElement(driver, trigger);

  const exactLabels = Array.isArray(labels) ? labels : [labels];
  for (const label of exactLabels) {
    try {
      const option = await waitForDisplayed(
        driver,
        By.xpath(`//*[@role='option' and contains(normalize-space(.), ${xpathLiteral(label)})]`),
        5000,
      );
      await clickElement(driver, option);
      return label;
    } catch {
      // Try the next candidate text.
    }
  }

  throw new Error(`Unable to select option for ${id}`);
}

async function clickWorkspaceCook(driver, recipeName) {
  const workspaceCook = By.xpath(
    `//*[normalize-space()=${xpathLiteral(recipeName)}]/ancestor::*[.//button[@title='Cook' or @aria-label='Cook']][1]//button[@title='Cook' or @aria-label='Cook']`,
  );
  try {
    const button = await waitForDisplayed(driver, workspaceCook, 10_000);
    await clickElement(driver, button);
    return "workspace";
  } catch {
    const mainCook = By.xpath(
      `//*[normalize-space()=${xpathLiteral(recipeName)}]/ancestor::*[.//button[normalize-space()='Cook']][1]//button[normalize-space()='Cook']`,
    );
    const button = await waitForDisplayed(driver, mainCook, 10_000);
    await clickElement(driver, button);
    return "main";
  }
}

function sshExec(command) {
  return execFileSync(
    "sshpass",
    [
      "-p",
      SSH_PASSWORD,
      "ssh",
      "-o",
      "StrictHostKeyChecking=no",
      "-o",
      "UserKnownHostsFile=/dev/null",
      "-o",
      "LogLevel=ERROR",
      "-p",
      String(SSH_PORT),
      `${SSH_USER}@${SSH_HOST}`,
      command,
    ],
    {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
}

function sshReadJson(remotePath) {
  return JSON.parse(sshExec(`cat ${remotePath}`));
}

function writePerfReport(report) {
  ensureDir(REPORT_DIR);
  fs.writeFileSync(
    path.join(REPORT_DIR, "perf-report.json"),
    JSON.stringify(report, null, 2),
  );
}

async function enterRemoteInstance(driver) {
  await waitForText(driver, "Recipe E2E Docker", 45_000);

  // Step 1: Click "Check" button on the instance card to initiate SSH connection
  console.log("Looking for Check button on instance card...");
  try {
    const checkBtn = await waitForDisplayed(
      driver,
      By.xpath(`//*[normalize-space()=${xpathLiteral("Recipe E2E Docker")}]/ancestor::*[contains(@class,'card') or @role='button' or @data-slot='card'][1]//button[normalize-space()='Check']`),
      10_000,
    );
    console.log("Clicking Check button to initiate SSH connection");
    await clickElement(driver, checkBtn);
  } catch {
    console.log("No Check button found, trying direct card click");
  }

  // Step 2: Wait for SSH connection to establish (checking spinner → green dot)
  console.log("Waiting for SSH connection to establish...");
  const sshDeadline = Date.now() + 90_000;
  let connected = false;
  while (Date.now() < sshDeadline) {
    const body = await pageText(driver);
    // Look for signs that SSH probe completed: agent count, healthy status, or model info
    if (body.includes("Checking")) {
      // Still checking, wait
      await sleep(driver, 2000);
      continue;
    }
    if (body.includes("agent") || body.includes("Agent") || body.includes("healthy") || body.includes("Gateway")) {
      console.log("SSH connection indicators found");
      connected = true;
      break;
    }
    await sleep(driver, 2000);
  }
  if (!connected) {
    console.log("WARNING: SSH connection indicators not detected, proceeding anyway");
  }

  // Step 3: Click the instance card to open it
  console.log("Opening instance tab...");
  const card = await waitForDisplayed(
    driver,
    By.xpath(`//*[normalize-space()=${xpathLiteral("Recipe E2E Docker")}]`),
    20_000,
  );
  await clickElement(driver, card);

  // Step 4: Wait for Home page to load with remote data
  await waitForAnyText(driver, ["Status", "Agents", "Home"], 60_000);
  console.log("Waiting for remote data to load on Home page...");
  const dataDeadline = Date.now() + 15_000;
  while (Date.now() < dataDeadline) {
    const body = await pageText(driver);
    if (body.includes("main") && (body.includes("anthropic") || body.includes("claude") || body.includes("Model") || body.includes("Sonnet"))) {
      console.log("Remote agent data loaded successfully");
      break;
    }
    await sleep(driver, 2000);
  }

  // Brief settle time
  await sleep(driver, 1000);
  console.log("Instance ready for recipe operations");

  // Debug: verify SSH is reachable from the test process
  try {
    const sshTest = sshExec("echo SSH_REACHABLE && cat /root/.openclaw/openclaw.json | head -3");
    console.log("SSH debug check:", sshTest.trim());
  } catch (e) {
    console.log("SSH debug check FAILED:", e.message);
  }
}

async function maybeApprove(driver) {
  const body = await pageText(driver);
  if (!body.includes("Approve and continue")) {
    return false;
  }
  await clickButtonText(driver, ["Approve and continue"], 15_000);
  await waitForAnyText(driver, ["Execute", "Back to configuration"], 20_000);
  return true;
}

async function runDedicatedAgent(driver) {
  const slug = "dedicated-agent";
  const recipeName = "Dedicated Agent";
  const timings = {};
  const totalStart = performance.now();

  await clickNav(driver, "Recipes");
  await waitForText(driver, "Workspace drafts", 20_000);

  const pageLoadStart = performance.now();
  await clickWorkspaceCook(driver, recipeName);
  await waitForDisplayed(driver, By.css("#agent_id"), 30_000);
  timings.page_load_ms = roundMs(performance.now() - pageLoadStart);

  await shot(driver, slug, "recipe-selected");

  const fillStart = performance.now();
  await fillById(driver, "agent_id", "test-e2e-agent");
  await selectByTriggerId(driver, "model", ["Use global default"]);
  await fillById(driver, "name", "E2E Test Agent");
  await fillById(driver, "persona", "You are a helpful test agent");
  timings.form_fill_ms = roundMs(performance.now() - fillStart);

  await shot(driver, slug, "form-filled");

  const executionStart = performance.now();
  await clickButtonText(driver, ["Next"], 10_000);
  await waitForAnyText(driver, ["Review what this recipe will do", "Planned changes", "change(s) to make"], 60_000);
  await maybeApprove(driver);
  await clickButtonText(driver, ["Execute"], 10_000);
  await waitForAnyText(
    driver,
    ["Created dedicated agent E2E Test Agent (test-e2e-agent)", "Your recipe changes were applied"],
    120_000,
  );
  timings.execution_ms = roundMs(performance.now() - executionStart);

  await shot(driver, slug, "execution-complete");

  const verificationStart = performance.now();
  await clickNav(driver, "Home");
  await waitForText(driver, "E2E Test Agent", 60_000);
  await waitForText(driver, "test-e2e-agent", 60_000);
  await shot(driver, slug, "agent-on-home");

  const remoteConfig = sshReadJson(REMOTE_CONFIG);
  const dedicatedAgent = (remoteConfig.agents?.list || []).find(
    (agent) => agent.id === "test-e2e-agent",
  );
  if (!dedicatedAgent) {
    throw new Error("Dedicated agent missing from remote openclaw.json");
  }

  const dedicatedIdentityPath = (
    dedicatedAgent.agentDir
    || dedicatedAgent.workspace
    || "/root/.openclaw/agents/test-e2e-agent/agent"
  ).replace(/\/$/, "");
  const identityText = sshExec(
    `cat ${dedicatedIdentityPath}/IDENTITY.md 2>/dev/null || true`,
  );
  if (!identityText.includes("E2E Test Agent")) {
    throw new Error("Dedicated agent IDENTITY.md missing display name");
  }
  if (!identityText.includes("You are a helpful test agent")) {
    throw new Error("Dedicated agent IDENTITY.md missing persona");
  }
  timings.verification_ms = roundMs(performance.now() - verificationStart);
  timings.total_ms = roundMs(performance.now() - totalStart);

  return {
    recipe_name: recipeName,
    ...timings,
  };
}

async function runChannelPersonaPack(driver) {
  const slug = "channel-persona-pack";
  const recipeName = "Channel Persona Pack";
  const timings = {};
  const totalStart = performance.now();

  await clickNav(driver, "Recipes");
  await waitForText(driver, recipeName, 20_000);

  const pageLoadStart = performance.now();
  await clickWorkspaceCook(driver, recipeName);
  await waitForDisplayed(driver, By.css("#guild_id"), 30_000);
  timings.page_load_ms = roundMs(performance.now() - pageLoadStart);

  await shot(driver, slug, "recipe-selected");

  const fillStart = performance.now();
  await selectByTriggerId(driver, "guild_id", ["Recipe Lab", "guild-recipe-lab"]);
  await sleep(driver, LONG_WAIT_MS);
  await selectByTriggerId(driver, "channel_id", ["support", "channel-support"]);
  await selectByTriggerId(driver, "persona_preset", ["Support Concierge"]);
  timings.form_fill_ms = roundMs(performance.now() - fillStart);

  await shot(driver, slug, "form-filled");

  const executionStart = performance.now();
  await clickButtonText(driver, ["Next"], 10_000);
  await waitForAnyText(driver, ["Review what this recipe will do", "Planned changes", "change(s) to make"], 60_000);
  await maybeApprove(driver);
  await clickButtonText(driver, ["Execute"], 10_000);
  await waitForAnyText(
    driver,
    ["Updated persona for channel channel-support", "Your recipe changes were applied"],
    120_000,
  );
  timings.execution_ms = roundMs(performance.now() - executionStart);

  await shot(driver, slug, "execution-complete");

  const verificationStart = performance.now();
  const remoteConfig = sshReadJson(REMOTE_CONFIG);
  const directPrompt =
    remoteConfig.channels?.discord?.guilds?.["guild-recipe-lab"]?.channels?.["channel-support"]?.systemPrompt;
  const accountPrompt =
    remoteConfig.channels?.discord?.accounts?.default?.guilds?.["guild-recipe-lab"]?.channels?.["channel-support"]?.systemPrompt;

  if (
    directPrompt?.trim?.() !== CHANNEL_SUPPORT_PERSONA
    && accountPrompt?.trim?.() !== CHANNEL_SUPPORT_PERSONA
  ) {
    throw new Error("Channel persona was not persisted to remote config");
  }
  timings.verification_ms = roundMs(performance.now() - verificationStart);
  timings.total_ms = roundMs(performance.now() - totalStart);

  return {
    recipe_name: recipeName,
    ...timings,
  };
}

async function runAgentPersonaPack(driver) {
  const slug = "agent-persona-pack";
  const recipeName = "Agent Persona Pack";
  const timings = {};
  const totalStart = performance.now();

  await clickNav(driver, "Recipes");
  await waitForText(driver, recipeName, 20_000);

  const pageLoadStart = performance.now();
  await clickWorkspaceCook(driver, recipeName);
  await waitForDisplayed(driver, By.css("#agent_id"), 30_000);
  timings.page_load_ms = roundMs(performance.now() - pageLoadStart);

  await shot(driver, slug, "recipe-selected");

  const fillStart = performance.now();
  await selectByTriggerId(driver, "agent_id", ["Main Agent", "main"]);
  await selectByTriggerId(driver, "persona_preset", ["Coach"]);
  timings.form_fill_ms = roundMs(performance.now() - fillStart);

  await shot(driver, slug, "form-filled");

  const executionStart = performance.now();
  await clickButtonText(driver, ["Next"], 10_000);
  await waitForAnyText(driver, ["Review what this recipe will do", "Planned changes", "change(s) to make"], 60_000);
  await maybeApprove(driver);
  await clickButtonText(driver, ["Execute"], 10_000);
  await waitForAnyText(
    driver,
    ["Updated persona for agent main", "Your recipe changes were applied"],
    120_000,
  );
  timings.execution_ms = roundMs(performance.now() - executionStart);

  await shot(driver, slug, "execution-complete");

  const verificationStart = performance.now();
  const identityText = sshExec(`cat ${REMOTE_IDENTITY_MAIN}`);
  if (!identityText.includes("Main Agent")) {
    throw new Error("Main agent IDENTITY.md lost its name");
  }
  if (!identityText.includes("🤖")) {
    throw new Error("Main agent IDENTITY.md lost its emoji");
  }
  if (!identityText.includes(AGENT_COACH_PERSONA)) {
    throw new Error("Main agent coach persona was not written");
  }
  timings.verification_ms = roundMs(performance.now() - verificationStart);
  timings.total_ms = roundMs(performance.now() - totalStart);

  return {
    recipe_name: recipeName,
    ...timings,
  };
}

async function main() {
  ensureDir(SCREENSHOT_DIR);
  ensureDir(REPORT_DIR);

  const report = {
    generated_at: new Date().toISOString(),
    app_binary: APP_BINARY,
    webdriver_url: "http://127.0.0.1:4444/",
    ssh_target: `${SSH_USER}@${SSH_HOST}:${SSH_PORT}`,
    recipes: [],
  };

  const caps = new Capabilities();
  caps.set("tauri:options", { application: APP_BINARY });
  caps.setBrowserName("wry");

  const driver = await new Builder()
    .withCapabilities(caps)
    .usingServer("http://127.0.0.1:4444/")
    .build();

  try {
    await waitForApp(driver);
    await enterRemoteInstance(driver);

    const recipes = [
      runDedicatedAgent,
      runChannelPersonaPack,
      runAgentPersonaPack,
    ];

    for (const recipeRun of recipes) {
      try {
        const result = await recipeRun(driver);
        report.recipes.push(result);
        writePerfReport(report);
      } catch (error) {
        const slug = recipeRun.name.replace(/^run/, "").replace(/[A-Z]/g, (m, i) => `${i ? "-" : ""}${m.toLowerCase()}`);
        await shot(driver, "errors", slug).catch(() => {});
        throw error;
      }
    }

    writePerfReport(report);
    console.log("Recipe GUI E2E finished successfully");
  } finally {
    writePerfReport(report);
    await driver.quit();
  }
}

main().catch((error) => {
  console.error("Fatal:", error);
  process.exit(1);
});
