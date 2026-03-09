import { describe, expect, test } from "bun:test";
import React from "react";

import {
  SelectContent,
  SelectItem,
  SelectLabel,
  SelectScrollDownButton,
  SelectScrollUpButton,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "../select";

describe("SelectContent", () => {
  test("uses popper positioning by default for stable overlay height", () => {
    const element = SelectContent({
      children: React.createElement("div", null, "content"),
    });
    const content = element.props.children;

    expect(content.props.position).toBe("popper");
    expect(content.props.align).toBe("center");
  });

  test("does not apply popper viewport sizing when item-aligned is requested explicitly", () => {
    const element = SelectContent({
      position: "item-aligned",
      children: React.createElement("div", null, "content"),
    });
    const content = element.props.children;
    const viewport = content.props.children[1];

    expect(content.props.position).toBe("item-aligned");
    expect(String(content.props.className)).not.toContain("translate-y-1");
    expect(String(viewport.props.className)).toBe("p-1");
  });
});

describe("Select wrappers", () => {
  test("marks trigger size and keeps the chevron icon", () => {
    const element = SelectTrigger({
      size: "sm",
      children: React.createElement(SelectValue, null),
    });

    expect(element.props["data-size"]).toBe("sm");
    expect(String(element.props.className)).toContain("data-[size=sm]:h-8");
    expect(element.props.children).toHaveLength(2);
  });

  test("applies label, item, separator, and scroll button classes", () => {
    const label = SelectLabel({ children: "Label" });
    const item = SelectItem({ value: "a", children: "Alpha" });
    const separator = SelectSeparator({});
    const scrollUp = SelectScrollUpButton({});
    const scrollDown = SelectScrollDownButton({});

    expect(String(label.props.className)).toContain("text-xs");
    expect(String(item.props.className)).toContain("cursor-default");
    expect(String(separator.props.className)).toContain("h-px");
    expect(String(scrollUp.props.className)).toContain("justify-center");
    expect(String(scrollDown.props.className)).toContain("justify-center");
  });
});
