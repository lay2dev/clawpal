/**
 * ClawPal Screenshot Harness — tauri-driver + Selenium
 * Captures every page and key interaction, organized by business flow.
 */
import fs from "fs";
import path from "path";
import { Builder, By, Capabilities } from "selenium-webdriver";

const SCREENSHOT_DIR = process.env.SCREENSHOT_DIR || "/screenshots";
const APP_BINARY = process.env.APP_BINARY || "/usr/local/bin/clawpal";
const BOOT_WAIT_MS = parseInt(process.env.BOOT_WAIT_MS || "8000", 10);
const NAV_WAIT_MS = 2000;
const CLICK_WAIT_MS = 1500;

function ensureDir(dir) { fs.mkdirSync(dir, { recursive: true }); }

async function shot(driver, category, name) {
  const dir = path.join(SCREENSHOT_DIR, category);
  ensureDir(dir);
  const png = await driver.takeScreenshot();
  fs.writeFileSync(path.join(dir, `${name}.png`), Buffer.from(png, "base64"));
  console.log(`  📸 ${category}/${name}.png`);
}

async function retryFind(driver, selector, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const els = await driver.findElements(By.css(selector));
      if (els.length > 0) return els;
    } catch (err) {
      // WebKitWebDriver can throw NoSuchFrame during page transitions
      // Retry silently
    }
    await driver.sleep(1000);
  }
  return [];
}

async function waitForApp(driver) {
  console.log("  Waiting for app to boot...");
  // Phase 1: wait for #root to have children (React mounted)
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    try {
      const root = await driver.findElements(By.css("#root > *"));
      if (root.length > 0) {
        console.log("  React root mounted");
        break;
      }
    } catch {
      // NoSuchFrame or other transient errors during boot — expected
    }
    await driver.sleep(1500);
  }
  // Phase 2: extra settle time for lazy components + data fetches
  await driver.sleep(BOOT_WAIT_MS);
}

async function clickNav(driver, label) {
  const buttons = await retryFind(driver, "aside nav button", 5000);
  for (const btn of buttons) {
    try {
      const text = await btn.getText();
      if (text.trim().toLowerCase().includes(label.toLowerCase())) {
        await btn.click();
        await driver.sleep(NAV_WAIT_MS);
        return true;
      }
    } catch { /* stale element */ }
  }
  console.warn(`  ⚠️  Nav "${label}" not found`);
  return false;
}

async function clickTab(driver, text) {
  const allBtns = await retryFind(driver, "div.flex.items-center button", 5000);
  for (const btn of allBtns) {
    try {
      const t = await btn.getText();
      if (t.includes(text)) { await btn.click(); await driver.sleep(CLICK_WAIT_MS); return true; }
    } catch { /* stale */ }
  }
  return false;
}

async function clickBtn(driver, text) {
  const buttons = await retryFind(driver, "button", 3000);
  for (const btn of buttons) {
    try {
      const t = await btn.getText();
      if (t.includes(text) && await btn.isDisplayed()) {
        await btn.click(); await driver.sleep(CLICK_WAIT_MS); return true;
      }
    } catch { /* stale */ }
  }
  return false;
}

async function scroll(driver, y) {
  try {
    await driver.executeScript(`document.querySelector('main')?.scrollTo(0, ${y})`);
    await driver.sleep(500);
  } catch { /* ignore scroll failures */ }
}

// ── Flow 1: Start Page (Control Center) ──
async function flowStartPage(driver) {
  console.log("\n📁 01-start-page/");
  await shot(driver, "01-start-page", "01-overview");
  if (await clickNav(driver, "Profiles")) await shot(driver, "01-start-page", "02-profiles");
  if (await clickNav(driver, "Settings")) await shot(driver, "01-start-page", "03-settings");
  await clickTab(driver, "Start");
  await driver.sleep(500);
}

// ── Flow 2: Home Dashboard ──
async function flowHome(driver) {
  console.log("\n📁 02-home/");
  await clickTab(driver, "Local");
  await driver.sleep(NAV_WAIT_MS);
  await shot(driver, "02-home", "01-dashboard");
  await scroll(driver, 500);
  await shot(driver, "02-home", "02-dashboard-scrolled");
  await scroll(driver, 0);
}

// ── Flow 3: Channels ──
async function flowChannels(driver) {
  console.log("\n📁 03-channels/");
  await clickNav(driver, "Channels");
  await shot(driver, "03-channels", "01-list");
  await scroll(driver, 500);
  await shot(driver, "03-channels", "02-list-scrolled");
  await scroll(driver, 0);
}

// ── Flow 4: Recipes ──
async function flowRecipes(driver) {
  console.log("\n📁 04-recipes/");
  await clickNav(driver, "Recipes");
  await shot(driver, "04-recipes", "01-list");
}

// ── Flow 5: Cron ──
async function flowCron(driver) {
  console.log("\n📁 05-cron/");
  await clickNav(driver, "Cron");
  await shot(driver, "05-cron", "01-list");
}

// ── Flow 6: Doctor ──
async function flowDoctor(driver) {
  console.log("\n📁 06-doctor/");
  await clickNav(driver, "Doctor");
  await shot(driver, "06-doctor", "01-main");
  await scroll(driver, 600);
  await shot(driver, "06-doctor", "02-scrolled");
  await scroll(driver, 0);
}

// ── Flow 7: Context ──
async function flowContext(driver) {
  console.log("\n📁 07-context/");
  await clickNav(driver, "Context");
  await shot(driver, "07-context", "01-main");
}

// ── Flow 8: History ──
async function flowHistory(driver) {
  console.log("\n📁 08-history/");
  await clickNav(driver, "History");
  await shot(driver, "08-history", "01-list");
}

// ── Flow 9: Chat Panel ──
async function flowChat(driver) {
  console.log("\n📁 09-chat/");
  await clickNav(driver, "Home");
  if (await clickBtn(driver, "Chat")) {
    await shot(driver, "09-chat", "01-open");
    // Close — find the X button in the chat aside
    try {
      const closeBtns = await driver.findElements(By.css("aside button"));
      for (const b of closeBtns) {
        try {
          const t = await b.getText();
          if (!t || t.trim() === "") { await b.click(); break; }
        } catch {}
      }
    } catch {}
    await driver.sleep(500);
  }
}

// ── Flow 10: Settings ──
async function flowSettings(driver) {
  console.log("\n📁 10-settings/");
  await clickTab(driver, "Start");
  await driver.sleep(500);
  if (await clickNav(driver, "Settings")) {
    await shot(driver, "10-settings", "01-main");
    await scroll(driver, 400); await shot(driver, "10-settings", "02-appearance");
    await scroll(driver, 800); await shot(driver, "10-settings", "03-advanced");
    await scroll(driver, 1200); await shot(driver, "10-settings", "04-bottom");
    await scroll(driver, 0);
  }
}

// ── Flow 11: Dark Mode ──
async function flowDarkMode(driver) {
  console.log("\n📁 11-dark-mode/");
  try {
    await driver.executeScript("localStorage.setItem('clawpal_theme','dark');document.documentElement.classList.add('dark');");
  } catch { /* retry after short wait */ await driver.sleep(1000); }
  await driver.navigate().refresh();
  await waitForApp(driver);

  await shot(driver, "11-dark-mode", "01-start-page");
  await clickTab(driver, "Local"); await driver.sleep(NAV_WAIT_MS);
  await shot(driver, "11-dark-mode", "02-home");
  await clickNav(driver, "Channels"); await shot(driver, "11-dark-mode", "03-channels");
  await clickNav(driver, "Doctor"); await shot(driver, "11-dark-mode", "04-doctor");
  await clickNav(driver, "Recipes"); await shot(driver, "11-dark-mode", "05-recipes");
  await clickNav(driver, "Cron"); await shot(driver, "11-dark-mode", "06-cron");
  await clickTab(driver, "Start"); await driver.sleep(500);
  await clickNav(driver, "Settings"); await shot(driver, "11-dark-mode", "07-settings");

  // Restore light
  try {
    await driver.executeScript("localStorage.setItem('clawpal_theme','light');document.documentElement.classList.remove('dark');");
  } catch {}
  await driver.navigate().refresh();
  await waitForApp(driver);
}

// ── Flow 12: Responsive ──
async function flowResponsive(driver) {
  console.log("\n📁 12-responsive/");
  const orig = await driver.manage().window().getRect();

  await driver.manage().window().setRect({ width: 1024, height: 680 });
  await driver.sleep(1500);
  await clickTab(driver, "Local"); await driver.sleep(NAV_WAIT_MS);
  await shot(driver, "12-responsive", "01-home-1024x680");
  if (await clickBtn(driver, "Chat")) {
    await shot(driver, "12-responsive", "02-chat-1024x680");
    try { await driver.actions().sendKeys("\uE00C").perform(); } catch {}
    await driver.sleep(500);
  }

  await driver.manage().window().setRect({ width: orig.width, height: orig.height });
  await driver.sleep(500);
}

// ── Flow 13: Dialogs ──
async function flowDialogs(driver) {
  console.log("\n📁 13-dialogs/");
  await clickTab(driver, "Local"); await driver.sleep(NAV_WAIT_MS);
  await clickNav(driver, "Home");
  if (await clickBtn(driver, "New Agent")) {
    await driver.sleep(800);
    await shot(driver, "13-dialogs", "01-create-agent");
    try { await driver.actions().sendKeys("\uE00C").perform(); } catch {}
    await driver.sleep(500);
  }
}

// ── Main ──
async function main() {
  ensureDir(SCREENSHOT_DIR);
  const caps = new Capabilities();
  caps.set("tauri:options", { application: APP_BINARY });
  caps.setBrowserName("wry");

  console.log("╔══════════════════════════════════════════╗");
  console.log("║  ClawPal Screenshot Harness (WebDriver)  ║");
  console.log("╚══════════════════════════════════════════╝");
  console.log(`Output: ${SCREENSHOT_DIR}\nBinary: ${APP_BINARY}\n`);

  const driver = await new Builder()
    .withCapabilities(caps)
    .usingServer("http://127.0.0.1:4444/")
    .build();

  try {
    await waitForApp(driver);
    console.log("✅ App booted\n");

    const flows = [
      ["Start Page", flowStartPage],
      ["Home", flowHome],
      ["Channels", flowChannels],
      ["Recipes", flowRecipes],
      ["Cron", flowCron],
      ["Doctor", flowDoctor],
      ["Context", flowContext],
      ["History", flowHistory],
      ["Chat", flowChat],
      ["Settings", flowSettings],
      ["Dark Mode", flowDarkMode],
      ["Responsive", flowResponsive],
      ["Dialogs", flowDialogs],
    ];

    let passed = 0, failed = 0;
    for (const [name, fn] of flows) {
      try { await fn(driver); passed++; }
      catch (err) {
        console.error(`\n❌ "${name}" failed: ${err.message}`);
        await shot(driver, "errors", `ERROR-${name.replace(/\s+/g, "-")}`).catch(() => {});
        failed++;
      }
    }

    // Summary
    console.log("\n════════════ Summary ════════════");
    let total = 0;
    const cats = fs.readdirSync(SCREENSHOT_DIR)
      .filter(f => fs.statSync(path.join(SCREENSHOT_DIR, f)).isDirectory()).sort();
    for (const cat of cats) {
      const files = fs.readdirSync(path.join(SCREENSHOT_DIR, cat)).filter(f => f.endsWith(".png")).sort();
      total += files.length;
      console.log(`  📁 ${cat}/ (${files.length})`);
      files.forEach(f => console.log(`      ${f}`));
    }
    console.log(`\n  Total: ${total} screenshots | ${passed} passed, ${failed} failed`);
    if (failed > 0) process.exit(1);
  } finally {
    await driver.quit();
  }
}

main().catch(err => { console.error("Fatal:", err); process.exit(1); });
