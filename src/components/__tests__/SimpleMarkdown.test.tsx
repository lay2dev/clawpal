import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { SimpleMarkdown } from "../SimpleMarkdown";

describe("SimpleMarkdown wrapping", () => {
  test("renders inline code with wrapping classes for long commands", () => {
    const html = renderToStaticMarkup(
      React.createElement(SimpleMarkdown, {
        content: "Run `clawpal doctor probe-openclaw --instance ssh:hetzner --tool-mode auto` now.",
      }),
    );

    expect(html).toContain("break-all");
    expect(html).toContain("whitespace-pre-wrap");
  });
});
