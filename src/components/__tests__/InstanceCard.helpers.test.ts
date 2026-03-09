import { describe, expect, test } from "bun:test";

import i18n from "@/i18n";
import {
  getConnectionQualityLabel,
  getConnectionStageLabel,
  getSshDotClass,
} from "../instance-card-helpers";

describe("InstanceCard helpers", () => {
  test("maps all ssh quality labels", async () => {
    await i18n.changeLanguage("en");

    expect(getConnectionQualityLabel("excellent", i18n.t.bind(i18n))).toBe("Excellent");
    expect(getConnectionQualityLabel("good", i18n.t.bind(i18n))).toBe("Good");
    expect(getConnectionQualityLabel("fair", i18n.t.bind(i18n))).toBe("Fair");
    expect(getConnectionQualityLabel("poor", i18n.t.bind(i18n))).toBe("Poor");
    expect(getConnectionQualityLabel("mystery", i18n.t.bind(i18n))).toBe("Unknown");
  });

  test("maps all ssh bottleneck stages", async () => {
    await i18n.changeLanguage("en");

    expect(getConnectionStageLabel("connect", i18n.t.bind(i18n))).toBe("TCP connect");
    expect(getConnectionStageLabel("gateway", i18n.t.bind(i18n))).toBe("Gateway check");
    expect(getConnectionStageLabel("config", i18n.t.bind(i18n))).toBe("Config fetch");
    expect(getConnectionStageLabel("agents", i18n.t.bind(i18n))).toBe("Agents fetch");
    expect(getConnectionStageLabel("version", i18n.t.bind(i18n))).toBe("Version check");
    expect(getConnectionStageLabel("other", i18n.t.bind(i18n))).toBe("Other");
  });

  test("maps ssh quality to the expected dot classes", () => {
    expect(getSshDotClass("excellent")).toContain("bg-emerald-500");
    expect(getSshDotClass("good")).toContain("bg-lime-500");
    expect(getSshDotClass("fair")).toContain("bg-amber-500");
    expect(getSshDotClass("poor")).toContain("bg-red-500");
    expect(getSshDotClass("unknown")).toBe("bg-muted-foreground/40");
  });
});
