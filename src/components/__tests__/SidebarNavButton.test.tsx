import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { BookOpenIcon } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { SidebarNavButton } from "../SidebarNavButton";

describe("SidebarNavButton", () => {
  test("renders a disabled nav item with hover title and badge", () => {
    const html = renderToStaticMarkup(
      React.createElement(SidebarNavButton, {
        item: {
          key: "recipes",
          active: false,
          disabled: true,
          tooltip: "coming soon",
          icon: React.createElement(BookOpenIcon, { className: "size-4" }),
          label: "Recipe",
          badge: React.createElement(Badge, { variant: "outline" }, "coming soon"),
          onClick: () => {},
        },
      }),
    );

    expect(html).toContain('aria-disabled="true"');
    expect(html).toContain('title="coming soon"');
    expect(html).toContain("cursor-not-allowed");
    expect(html).toContain("Recipe");
    expect(html).toContain("coming soon");
  });
});
