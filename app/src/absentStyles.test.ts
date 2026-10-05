import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * **The rules that lay out "Not cloned here", in the explorer and in Settings › Repos** (#1215,
 * #1228), read from the stylesheet itself. jsdom applies no stylesheet, so a vitest of the
 * rendered lists cannot see a rule that went missing; this reads `App.css` the way the
 * explorer's guide test does (`Explorer.test.tsx`).
 *
 * It exists because a rule was once pasted into the middle of another rule's selector: still
 * valid CSS, so nothing failed, but the explorer row's buttons fell below their line and the
 * Settings list drew bullets. So: each rule is found by its exact selector, and no selector has
 * a comment inside it.
 */

/** The stylesheet, each comment cut down to a bare marker, so the braces a comment may quote
 *  cannot split a rule. Where a comment was is still visible. */
const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
  /\/\*[\s\S]*?\*\//g,
  "/**/",
);

/** Every flat rule as [selector, body], the comments before a selector taken off it. */
const rules: [string, string][] = [...css.matchAll(/([^{}]*)\{([^{}]*)\}/g)].map(
  ([, selector, body]) => [
    selector
      .replace(/^(\s*\/\*\*\/)*/, "")
      .replace(/\s+/g, " ")
      .trim(),
    body,
  ],
);

/** The declarations of the rule with exactly this selector, joined. */
function declared(selector: string): string {
  return rules
    .filter(([one]) => one === selector)
    .map(([, body]) => body.replace(/\s+/g, " "))
    .join(";");
}

describe("the stylesheet for repos that are not cloned here", () => {
  it("keeps the explorer row's actions on the row's line", () => {
    expect(declared(".explorer .absent-actions")).toContain("margin-left: auto");
    expect(declared(".explorer .absent-actions .ui-setting-actions")).toContain("margin: 0");
  });

  it("draws Settings' list as plain lines, its Remove on each line", () => {
    const list = declared(".absent-list");
    expect(list).toContain("list-style: none");
    expect(list).toContain("padding: 0");
    expect(declared(".absent-list li")).toContain("display: flex");
    expect(declared(".absent-list .ui-setting-actions")).toContain("margin: 0");
  });

  it("has no comment inside a selector, where a pasted rule would hide", () => {
    expect(rules.filter(([selector]) => selector.includes("/*")).map(([s]) => s)).toEqual([]);
  });
});
