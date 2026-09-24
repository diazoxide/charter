import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * **Nothing in the window rubber-bands.** The operator saw the whole window's chrome pulled
 * down by macOS's elastic overscroll and asked for it gone. jsdom computes no stylesheet, so
 * this reads the rule as text (the same way `ChatGauge.test.tsx` does): the universal rule
 * must switch overscroll off for every element, the page included.
 */
describe("overscroll", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
    /\/\*[\s\S]*?\*\//g,
    "",
  );

  it("is off for every element, so neither the window nor a region bounces", () => {
    const universal = /(^|\})\s*\*\s*\{([^}]*)\}/.exec(css);
    expect(universal, "a `* { … }` rule").not.toBeNull();
    expect(universal?.[2]).toMatch(/overscroll-behavior:\s*none\s*;/);
  });

  it("is not turned back on anywhere", () => {
    expect(css).not.toMatch(/overscroll-behavior:\s*(auto|contain)/);
  });
});
