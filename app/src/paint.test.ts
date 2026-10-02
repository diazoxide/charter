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
 * jsdom computes no stylesheet, so this reads the rules as text, as `overscroll.test.ts` does —
 * **every** rule whose selector list names one of these, wherever it is in the file and however
 * it is grouped, `@media` blocks included.
 */
describe("a chat's row and its mark", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
    /\/\*[\s\S]*?\*\//g,
    "",
  );

  /** Every innermost `selectors { declarations }` in the file. */
  const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(([, selectors, body]) => ({
    selectors: selectors.trim(),
    body,
  }));

  /** The class tokens of one selector, exactly: `.state` is not `.state-bar`. */
  const classes = (selector: string) => new Set(selector.match(/\.[\w-]+/g) ?? []);
  const MARKS = [
    ".state",
    ".state-unknown",
    ".state-done",
    ".state-running",
    ".state-waiting",
    ".state-failed",
  ];
  /** Whether one selector of a list draws something there is one of per chat. */
  const perChat = (selector: string) => {
    const has = classes(selector);
    return (
      MARKS.some((mark) => has.has(mark)) ||
      (has.has(".explorer") && (has.has(".chat") || has.has(".unreported")))
    );
  };
  const drawn = rules.filter((rule) => rule.selectors.split(",").some(perChat));

  it("finds the rules it is about", () => {
    const named = drawn.map((rule) => rule.selectors);
    for (const selector of [
      ".explorer .chat",
      ".explorer .unreported",
      ".state",
      ".state-unknown",
      ".state-done",
    ])
      expect(named, selector).toContain(selector);
  });

  it("is dimmed in its colour, never by a layer of its own", () => {
    for (const { selectors, body } of drawn) {
      expect(body, selectors).not.toMatch(/(^|[;\s])opacity\s*:/);
      expect(body, selectors).not.toMatch(/filter\s*:[^;]*opacity\(/);
    }
  });

  it("draws no dashed or dotted border", () => {
    for (const { selectors, body } of drawn)
      expect(body, selectors).not.toMatch(/\b(dashed|dotted)\b/);
  });

  it("says the plain colour first, for a WebView without color-mix", () => {
    for (const { selectors, body } of drawn) {
      const declarations = body
        .split(";")
        .map((one) => one.trim())
        .filter(Boolean)
        .map((one) => {
          const at = one.indexOf(":");
          return { property: one.slice(0, at).trim(), value: one.slice(at + 1).trim() };
        });
      declarations.forEach(({ property, value }, at) => {
        if (!value.includes("color-mix(")) return;
        const plain = declarations
          .slice(0, at)
          .some((before) => before.property === property && !before.value.includes("color-mix("));
        expect(plain, `${selectors}: a plain ${property} before the color-mix one`).toBe(true);
      });
    }
    // And the four that are dimmed this way are dimmed this way.
    for (const selector of [
      ".explorer .chat",
      ".explorer .unreported",
      ".state-unknown",
      ".state-done",
    ])
      expect(drawn.find((rule) => rule.selectors === selector)?.body, selector).toMatch(
        /color-mix\(/,
      );
  });

  it("still tells unknown from done by shape: a broken ring against a whole one", () => {
    const body = (selector: string) =>
      drawn.find((rule) => rule.selectors === selector)?.body ?? "";
    expect(body(".state-unknown")).toMatch(/border-color:\s*color-mix\([^;]*\)\s+transparent\s*;/);
    expect(body(".state-done")).not.toMatch(/transparent\s*;/);
  });
});
