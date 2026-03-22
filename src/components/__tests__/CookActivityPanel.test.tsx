import { describe, expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";
import i18n from "@/i18n";
import { CookActivityPanel } from "@/components/CookActivityPanel";
import type { RecipeRuntimeAuditEntry } from "@/lib/types";

function renderPanel(
  activities: RecipeRuntimeAuditEntry[],
  open = true,
): string {
  return renderToStaticMarkup(
    <I18nextProvider i18n={i18n}>
      <CookActivityPanel
        title="Allowed activity"
        description="Review the concrete commands and internal steps that ran for this recipe."
        activities={activities}
        open={open}
        onOpenChange={() => {}}
      />
    </I18nextProvider>,
  );
}

function makeActivity(
  overrides: Partial<RecipeRuntimeAuditEntry> = {},
): RecipeRuntimeAuditEntry {
  return {
    id: "audit-1",
    phase: "execute",
    kind: "command",
    label: "Apply recipe step",
    status: "succeeded",
    sideEffect: false,
    startedAt: "2026-03-22T10:00:00.000Z",
    ...overrides,
  };
}

describe("CookActivityPanel", () => {
  test("renders open activity list in chronological order with summary badges", () => {
    const markup = renderPanel([
      makeActivity({
        id: "audit-late",
        label: "Later step",
        startedAt: "2026-03-22T10:00:05.000Z",
      }),
      makeActivity({
        id: "audit-early",
        label: "Earlier step",
        startedAt: "2026-03-22T10:00:01.000Z",
        sideEffect: true,
        target: "ssh:hetzner",
        exitCode: 0,
      }),
    ]);

    expect(markup).toContain("Allowed activity");
    expect(markup).toContain("Succeeded");
    expect(markup).toContain("Side effect");
    expect(markup).toContain("ssh:hetzner");
    expect(markup).toContain("Exit code 0");
    expect(markup.indexOf("Earlier step")).toBeLessThan(markup.indexOf("Later step"));
  });

  test("renders empty state when there is no recorded activity", () => {
    const markup = renderPanel([]);

    expect(markup).toContain("No activity recorded yet.");
  });

  test("keeps item details hidden by default and when the panel is collapsed", () => {
    const activity = makeActivity({
      id: "audit-failed",
      status: "failed",
      label: "Resolve provider credentials",
      exitCode: 17,
      displayCommand: "openclaw auth inspect --json",
      stderrSummary: "provider auth failed",
      details: "The provider profile is missing a usable credential.",
    });

    const openMarkup = renderPanel([activity], true);
    const collapsedMarkup = renderPanel([activity], false);

    expect(openMarkup).toContain("Failed");
    expect(openMarkup).toContain("Exit code 17");
    expect(openMarkup).not.toContain("openclaw auth inspect --json");
    expect(openMarkup).not.toContain("provider auth failed");
    expect(openMarkup).not.toContain(
      "The provider profile is missing a usable credential.",
    );
    expect(collapsedMarkup).not.toContain("Resolve provider credentials");
    expect(collapsedMarkup).not.toContain("Failed");
  });
});
