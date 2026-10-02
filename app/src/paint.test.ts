import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * **What a chat draws once per chat is cheap to paint** (#891, FR-27's L9).
 *
 * A project with fifty chats lists fifty rows in its explorer, each with a state mark, and the
 * same marks sit on its tabs. On CI's software-rendered WebViews those rows were most of what
 * the first switch into such a project cost: an `opacity` below 1 gives each element its own
 * transparency layer to paint and blend, a row's layer held its mark's layer inside it, and a
 * dashed round border is stroked dash by dash. So the dimming is mixed into the colour instead,
 * and the ring that says `unknown` is two solid arcs rather than a dashed circle.
 *
 * jsdom computes no stylesheet, so this reads the rules as text, as `overscroll.test.ts` does.
 */
describe("a chat's row and its mark", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
    /\/\*[\s\S]*?\*\//g,
    "",
  );
  const rule = (selector: string) => {
    const found = new RegExp(
      `(?:^|\\})\\s*${selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\{([^}]*)\\}`,
    ).exec(css);
    expect(found, `a \`${selector} { … }\` rule`).not.toBeNull();
    return found?.[1] ?? "";
  };
  const PER_CHAT = [".explorer .chat", ".state", ".state-unknown", ".state-done"];

  it("is dimmed in its colour, never by a layer of its own", () => {
    for (const selector of PER_CHAT) expect(rule(selector), selector).not.toMatch(/opacity\s*:/);
  });

  it("draws no dashed or dotted border", () => {
    for (const selector of PER_CHAT)
      expect(rule(selector), selector).not.toMatch(/\b(dashed|dotted)\b/);
  });

  it("still tells unknown from done by shape: a broken ring against a whole one", () => {
    expect(rule(".state-unknown")).toMatch(/border-color:\s*color-mix\([^;]*\)\s+transparent\s*;/);
    expect(rule(".state-done")).not.toMatch(/transparent\s*;/);
  });
});
