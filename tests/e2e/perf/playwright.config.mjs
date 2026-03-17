import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: "home-perf.spec.mjs",
  timeout: 60_000,
  retries: 0,
  use: {
    headless: true,
    viewport: { width: 1280, height: 720 },
  },
});
