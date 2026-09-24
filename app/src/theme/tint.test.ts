/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  BUILT_IN,
  DEFAULT_THEME,
  drawIn,
  drawTint,
  property,
  TINTED_TABS,
  TINTED_WINDOW,
  TOKENS,
  tinted,
  tintVariables,
} from "./theme";
import { hueOf, PALETTE, tintHex } from "./tint";

/**
 * **A workspace's colour** (charter-app#281): a hue that tints the accent and the tab shades of
 * the theme in force, and nothing else. `contrast.test.ts` holds every palette hue on both
 * built-ins to the same floors as the themes themselves; this file holds what the tint touches.
 */

/** WCAG relative luminance of `#rrggbb`. */
function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((at) => {
    const v = Number.parseInt(hex.slice(at, at + 2), 16) / 255;
    return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The smallest angle between two hues, in degrees. */
function apart(a: number, b: number): number {
  const d = Math.abs(a - b) % 360;
  return Math.min(d, 360 - d);
}

describe("the palette", () => {
  it("names the eight colours the core reads", () => {
    const core = join(
      process.cwd(),
      "..",
      "crates",
      "charter-core",
      "src",
      "extension",
      "project",
      "theme.rs",
    );
    const source = readFileSync(core, "utf8");
    const declared = /PALETTE:\s*\[&str;\s*\d+\]\s*=\s*\[([^\]]*)\]/.exec(source);
    expect(declared, `PALETTE was not found in ${core}`).not.toBeNull();
    const named = [...(declared?.[1] ?? "").matchAll(/"([^"]+)"/g)].map((hit) => hit[1]);
    expect(named).toEqual(Object.keys(PALETTE));
  });

  it("reads a name as its hue, a #rrggbb as the hue it has, and anything else as none", () => {
    expect(hueOf("teal")).toBe(PALETTE.teal);
    expect(hueOf(tintHex("#6a9fb5", PALETTE.purple))).toBeCloseTo(PALETTE.purple, 0);
    for (const none of [null, undefined, "", "mauve", "#fff", "#12345", "#808080"])
      expect(hueOf(none), String(none)).toBeUndefined();
  });
});

describe("a tint", () => {
  it.each(Object.keys(BUILT_IN))(
    "turns only %s's accent and tab shades, and never its text or its terminal",
    (name) => {
      const theme = BUILT_IN[name];
      for (const colour of Object.keys(PALETTE)) {
        const tint = tinted(theme, colour);
        const moved = TOKENS.filter((token) => tint.values[token] !== theme.values[token]);
        expect(
          moved.filter((token) => !TINTED_TABS.includes(token)),
          colour,
        ).toEqual([]);
        expect(tint.name).toBe(theme.name);
        expect(tint.appearance).toBe(theme.appearance);
      }
    },
  );

  it.each(Object.keys(BUILT_IN))("gives %s's accent the colour's hue", (name) => {
    const theme = BUILT_IN[name];
    for (const [colour, hue] of Object.entries(PALETTE)) {
      const accent = tinted(theme, colour).values["accent.base"];
      // Within a few degrees: the hex is rounded to eight bits a channel.
      expect(apart(hueOf(accent) ?? Number.NaN, hue), `${colour}: ${accent}`).toBeLessThan(4);
    }
  });

  it.each(Object.keys(BUILT_IN))("keeps the luminance of every shade it turns in %s", (name) => {
    const theme = BUILT_IN[name];
    for (const colour of Object.keys(PALETTE)) {
      const tint = tinted(theme, colour);
      for (const token of TINTED_TABS) {
        const was = luminance(theme.values[token]);
        const now = luminance(tint.values[token]);
        expect(Math.abs(now - was) / Math.max(was, 0.01), `${colour} ${token}`).toBeLessThan(0.03);
      }
    }
  });

  it("colours a grey shade, which has no hue of its own", () => {
    const shade = DEFAULT_THEME.values["layer.chat"];
    expect(tinted(DEFAULT_THEME, "green").values["layer.chat"]).not.toBe(shade);
    expect(hueOf(tinted(DEFAULT_THEME, "green").values["layer.chat"])).toBeDefined();
  });

  it("is nothing without a colour, or with one charter does not read", () => {
    expect(tinted(DEFAULT_THEME, null)).toBe(DEFAULT_THEME);
    expect(tinted(DEFAULT_THEME, "mauve")).toBe(DEFAULT_THEME);
    expect(tintVariables(DEFAULT_THEME, null, TINTED_TABS)).toEqual({});
  });

  it("takes a custom colour's hue", () => {
    const custom = "#c06020";
    const hue = hueOf(custom) ?? Number.NaN;
    const accent = tinted(DEFAULT_THEME, custom).values["accent.base"];
    expect(apart(hueOf(accent) ?? Number.NaN, hue)).toBeLessThan(4);
    expect(accent).not.toBe(DEFAULT_THEME.values["accent.base"]);
  });

  it("is the same theme object each time it is asked, so a redraw repaints nothing", () => {
    expect(tinted(DEFAULT_THEME, "blue")).toBe(tinted(DEFAULT_THEME, "blue"));
  });

  it("is handed to an element as the custom properties of the tokens asked for", () => {
    const tint = tinted(DEFAULT_THEME, "pink");
    expect(tintVariables(DEFAULT_THEME, "pink", ["layer.chat", "accent.base"])).toEqual({
      [property("layer.chat")]: tint.values["layer.chat"],
      [property("accent.base")]: tint.values["accent.base"],
    });
  });
});

describe("the window's tint", () => {
  afterEach(() => {
    drawTint(null);
    drawIn(DEFAULT_THEME);
  });

  it("puts the workspace in front's accent and focus ring on the window, and no tab shade", () => {
    const root = document.documentElement;
    drawIn(DEFAULT_THEME);
    drawTint("yellow");
    const tint = tinted(DEFAULT_THEME, "yellow");
    for (const token of TINTED_WINDOW)
      expect(root.style.getPropertyValue(property(token)), token).toBe(tint.values[token]);
    // The tab shades are each tab's own, set on the tab: the project strip is not a workspace's.
    for (const token of ["layer.project", "layer.workspace", "layer.chat"] as const)
      expect(root.style.getPropertyValue(property(token)), token).toBe(
        DEFAULT_THEME.values[token],
      );
  });

  it("follows the theme drawn under it, and goes when the workspace has no colour", () => {
    const root = document.documentElement;
    drawTint("red");
    drawIn(BUILT_IN["charter-light"]);
    expect(root.style.getPropertyValue(property("accent.base"))).toBe(
      tinted(BUILT_IN["charter-light"], "red").values["accent.base"],
    );
    drawTint(null);
    expect(root.style.getPropertyValue(property("accent.base"))).toBe(
      BUILT_IN["charter-light"].values["accent.base"],
    );
  });
});
