import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

import {
  QuickDiagnoseDialog,
  handleQuickDiagnoseDialogOpenChange,
} from "../QuickDiagnoseDialog";

describe("QuickDiagnoseDialog", () => {
  test("renders when open=true", () => {
    const html = renderToStaticMarkup(
      React.createElement(QuickDiagnoseDialog, {
        open: true,
        onOpenChange: () => {},
        context: "connection timeout",
      }),
    );
    expect(html.includes("Quick Diagnose") || html.includes("quickDiagnose.title")).toBe(true);
  });

  test("does not render when open=false", () => {
    const html = renderToStaticMarkup(
      React.createElement(QuickDiagnoseDialog, {
        open: false,
        onOpenChange: () => {},
      }),
    );
    expect(html).toBe("");
  });

  test("calls onOpenChange(false) on close", () => {
    const calls: boolean[] = [];
    handleQuickDiagnoseDialogOpenChange((open) => calls.push(open), false);
    expect(calls).toEqual([false]);
  });
});
