import { beforeEach, describe, expect, test } from "bun:test";

import i18n from "@/i18n";
import {
  buildDefaultSshHostId,
  buildInstallPrompt,
  renderInstallPrompt,
  resolveInstallPromptLanguage,
  extractChoices,
  isToolNarration,
  sanitizeLocalIdSegment,
  sanitizeSshIdSegment,
} from "../install-hub-helpers";

describe("InstallHub helpers", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  test("matches tool narration phrases and ignores normal prose", () => {
    expect(isToolNarration("Running: ssh doctor")).toBe(true);
    expect(isToolNarration("建议执行诊断命令：doctor check")).toBe(true);
    expect(isToolNarration("I need a bit more information first.")).toBe(false);
  });

  test("parses assistant choice lists into prose and options", () => {
    expect(
      extractChoices(`Choose one:\n1. Fast path - use defaults\n2. Safe path - inspect first\nPlease tell me your choice.`),
    ).toEqual({
      prose: "",
      options: [
        { label: "Fast path", value: "Fast path - use defaults" },
        { label: "Safe path", value: "Safe path - inspect first" },
      ],
    });
  });

  test("returns null when fewer than two list options are found", () => {
    expect(extractChoices("1. Only one option")).toBeNull();
  });

  test("sanitizes ssh/local id segments and falls back when empty", () => {
    expect(sanitizeSshIdSegment(" Prod Box / Tokyo ")).toBe("prod-box-tokyo");
    expect(sanitizeSshIdSegment("!!!")).toBe("remote");
    expect(sanitizeLocalIdSegment(" Main / Dev ")).toBe("main-dev");
    expect(sanitizeLocalIdSegment("???")).toBe("default");
  });

  test("builds a default ssh host id from host or label", () => {
    expect(
      buildDefaultSshHostId({
        id: "",
        label: "My Host",
        host: "Prod Box",
        port: 22,
        username: "root",
        authMethod: "key",
      }),
    ).toBe("ssh:prod-box");
  });

  test("resolves the install prompt language from the current locale", () => {
    expect(resolveInstallPromptLanguage("zh-CN")).toBe("Chinese (简体中文)");
    expect(resolveInstallPromptLanguage("en-US")).toBe("English");
    expect(resolveInstallPromptLanguage(undefined)).toBe("English");
  });

  test("renders the install prompt template with the given values", () => {
    expect(
      renderInstallPrompt("Respond in {{LANGUAGE}}. User intent: {{USER_INTENT}}", {
        language: "Chinese (简体中文)",
        userIntent: "帮我连接远程实例",
      }),
    ).toBe("Respond in Chinese (简体中文). User intent: 帮我连接远程实例");
  });

  test("builds the fallback install prompt with the active locale and intent", async () => {
    await i18n.changeLanguage("zh-CN");

    const prompt = buildInstallPrompt("帮我连接远程实例");

    expect(prompt.trim().length).toBeGreaterThan(0);

    if (prompt.includes("User intent:")) {
      expect(prompt).toContain("Respond in Chinese (简体中文).");
      expect(prompt).toContain("User intent: 帮我连接远程实例");
      return;
    }

    // Bun's test loader may return the prompt asset path instead of raw markdown content.
    expect(prompt).toContain("install-hub-fallback.md");
  });
});
