import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { buildNavItems } from "../useNavItems";
import type { Route } from "@/lib/routes";

const translations: Record<string, string> = {
  "nav.home": "Home",
  "nav.channels": "Channels",
  "nav.recipes": "Recipe",
  "nav.cron": "Cron",
  "nav.doctor": "Doctor",
  "nav.context": "Context",
  "nav.history": "History",
  "doctor.comingSoon": "coming soon",
};

describe("buildNavItems", () => {
  test("returns a disabled recipe placeholder with coming soon affordances", () => {
    const navigated: Route[] = [];
    const items = buildNavItems({
      inStart: false,
      startSection: "overview",
      setStartSection: () => {},
      route: "home",
      navigateRoute: (route) => navigated.push(route),
      openDoctor: () => {},
      doctorNavPulse: false,
      t: (key: string) => translations[key] ?? key,
    });

    const recipeItem = items.find((item) => item.key === "recipes");
    const badgeHtml = renderToStaticMarkup(React.createElement(React.Fragment, null, recipeItem?.badge));

    expect(recipeItem).toBeDefined();
    expect(recipeItem?.label).toBe("Recipe");
    expect(recipeItem?.disabled).toBe(true);
    expect(recipeItem?.tooltip).toBe("coming soon");
    expect(recipeItem?.active).toBe(false);
    expect(badgeHtml).toContain("coming soon");

    recipeItem?.onClick();
    expect(navigated).toEqual([]);
  });

  test("keeps the recipe placeholder highlighted while recipe routes are active", () => {
    const items = buildNavItems({
      inStart: false,
      startSection: "overview",
      setStartSection: () => {},
      route: "recipes",
      navigateRoute: () => {},
      openDoctor: () => {},
      doctorNavPulse: false,
      t: (key: string) => translations[key] ?? key,
    });

    expect(items.find((item) => item.key === "recipes")?.active).toBe(true);
  });
});
