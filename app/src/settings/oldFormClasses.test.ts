import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * **The old hand-built form classes stay gone** (DS-3e, #1177; the *contract* step of DS-3 #626,
 * V89f).
 *
 * Every form in the window is drawn from the settings set (`settings/components.tsx`): a row is
 * a `SettingRow`, a box a `Field`, a pick a `Choice`, and a form's buttons a `SettingActions`
 * (V89j). Before it, two hand-built styles drew the same row and had drifted: `settings-*` on the
 * settings pages, and `asks` / `choices` / `choice` / `who` / `picking` in the dialogs. They were
 * deleted once nothing used them. A screen that reaches for one again would bring the drift back,
 * so this fails on any rule in `App.css` whose selector names one, and on any `className` in the
 * source that does.
 *
 * jsdom computes no stylesheet, so the rules are read as text, as `paint.test.ts` does.
 */

/** The deleted classes, exactly: `.choice` is not `.ui-choice-radio`. */
const GONE = ["asks", "choices", "choices-name", "choice", "who", "picking"];

/** The old settings family was a prefix: `settings-field`, `settings-hint`, `settings-actions`… */
const gone = (name: string) => GONE.includes(name) || name.startsWith("settings-");

const SRC = join(process.cwd(), "src");

/** Every source file the window is drawn from: not tests, which name the classes to deny them. */
function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return sources(path);
    return /\.tsx?$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name) ? [path] : [];
  });
}

/** The class names a `className` attribute can hold: every string in its value. */
function classNames(source: string): string[] {
  const values = [
    ...source.matchAll(/className=(?:"([^"]*)"|\{([^{}]*(?:\{[^{}]*\}[^{}]*)*)\})/g),
  ].flatMap(([, plain, expression]) =>
    plain !== undefined
      ? [plain]
      : [...(expression ?? "").matchAll(/"([^"]*)"|'([^']*)'|`([^`]*)`/g)].map(([, a, b, c]) =>
          (a ?? b ?? c ?? "").replace(/\$\{[^}]*\}/g, " "),
        ),
  );
  return values.flatMap((value) => value.split(/\s+/)).filter(Boolean);
}

describe("the old hand-built form classes", () => {
  it("have no rule in the stylesheet", () => {
    const css = readFileSync(join(SRC, "App.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
    const named = [...css.matchAll(/([^{}]+)\{[^{}]*\}/g)]
      .map(([, selectors]) => selectors.trim())
      .filter((selectors) =>
        (selectors.match(/\.[\w-]+/g) ?? []).some((token) => gone(token.slice(1))),
      );
    expect(named).toEqual([]);
  });

  it("are named by no className in the source", () => {
    const used = sources(SRC).flatMap((path) =>
      classNames(readFileSync(path, "utf8"))
        .filter(gone)
        .map((name) => `${relative(SRC, path)}: ${name}`),
    );
    expect(used).toEqual([]);
  });

  it("would be caught if one came back", () => {
    // The reader above is what the guard rests on, so it is held to the shapes a screen writes.
    expect(classNames(`<div className="asks" />`)).toEqual(["asks"]);
    expect(classNames(`<li className={on ? "choice on" : "row"} />`)).toEqual([
      "choice",
      "on",
      "row",
    ]);
    expect(classNames("<p className={`who ${extra}`} />")).toEqual(["who"]);
    expect(classNames(`<div className="ui-choice-radio" />`).filter(gone)).toEqual([]);
  });
});
