import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * **The buttons at the end of a task's line in a tab's menu** (#1489, #1494, #1551), read from
 * the stylesheet itself: jsdom applies none, so a window test cannot see when they are drawn.
 * As `absentStyles.test.ts` does, each rule is found by its exact selector.
 */

/** The stylesheet, each comment cut down to a bare marker. */
const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
  /\/\*[\s\S]*?\*\//g,
  "/**/",
);

/** Every flat rule's selectors, one by one, with the declarations of its rule. */
const selectors: [string, string][] = [...css.matchAll(/([^{}]*)\{([^{}]*)\}/g)].flatMap(
  ([, selector, body]) =>
    selector
      .replace(/^(\s*\/\*\*\/)*/, "")
      .split(",")
      .map((one): [string, string] => [one.replace(/\s+/g, " ").trim(), body]),
);

/** Whether a rule with exactly this selector among its selectors declares `declaration`. */
function declares(selector: string, declaration: string): boolean {
  return selectors.some(
    ([one, body]) => one === selector && body.replace(/\s+/g, " ").includes(declaration),
  );
}

describe("the buttons at the end of a task's line in a tab's menu", () => {
  it("are hidden until the line is reached", () => {
    expect(declares(".tasks-menu-places", "display: none")).toBe(true);
  });

  it("are drawn for the pointer on the line, and for the keyboard on it too", () => {
    for (const on of [":hover", "[data-highlighted]", ":focus-within"]) {
      expect(declares(`.tasks-menu-row${on} .tasks-menu-places`, "display: inline-flex"), on).toBe(
        true,
      );
    }
  });
});
