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
 * so this fails on any rule in a stylesheet under `src/` whose selector names one — `App.css`,
 * `styles.css`, and any added later — and on any `className` in the source that does.
 *
 * jsdom computes no stylesheet, so the rules are read as text, as `paint.test.ts` does.
 */

/** The deleted classes, exactly: `.choice` is not `.ui-choice-radio`. */
const GONE = ["asks", "choices", "choices-name", "choice", "who", "picking"];

/** The old settings family was a prefix: `settings-field`, `settings-hint`, `settings-actions`… */
const gone = (name: string) => GONE.includes(name) || name.startsWith("settings-");

const SRC = join(process.cwd(), "src");

/** Every file under `dir` whose name `keep` takes. */
function files(dir: string, keep: (name: string) => boolean): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return files(path, keep);
    return keep(entry.name) ? [path] : [];
  });
}

/** Every source file the window is drawn from: not tests, which name the classes to deny them. */
const sources = (dir: string) =>
  files(dir, (name) => /\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name));

/** Every stylesheet the window can import. */
const stylesheets = (dir: string) => files(dir, (name) => name.endsWith(".css"));

/** The selectors in `css` that name one of the old classes. */
function oldRules(css: string): string[] {
  return [...css.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}]+)\{[^{}]*\}/g)]
    .map(([, selectors]) => selectors.trim())
    .filter((selectors) =>
      (selectors.match(/\.[\w-]+/g) ?? []).some((token) => gone(token.slice(1))),
    );
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
  it("have no rule in any stylesheet", () => {
    const sheets = stylesheets(SRC);
    // App.css and styles.css at least: a reader that found none would pass on nothing.
    expect(sheets.map((path) => relative(SRC, path))).toEqual(
      expect.arrayContaining(["App.css", "styles.css"]),
    );
    const named = sheets.flatMap((path) =>
      oldRules(readFileSync(path, "utf8")).map((rule) => `${relative(SRC, path)}: ${rule}`),
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
    expect(oldRules(".asks {}\n.ui-choice-option {}\n.warning .choice label {}")).toEqual([
      ".asks",
      ".warning .choice label",
    ]);
  });
});
