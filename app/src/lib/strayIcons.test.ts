import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * **One icon family, and no stray icons** (DS-4, #627).
 *
 * The window draws an icon from Lucide (`lucide-react`, `docs/design-system.md`), or a file's
 * from the icon theme (`FileIcon.tsx`). Before this, three marks were text: the Doctor's ✓ ! ✗,
 * the Curate submenu's ▸ and the settings box's ✓. A glyph is the font's, so it is drawn in the
 * font's weight and size, sits on the font's baseline and changes with the font — none of which
 * an icon beside it does. This fails on the shapes a stray icon comes back in:
 *
 * - **a glyph drawn as an icon**: a JSX text that is only a glyph from {@link GLYPHS}, or a
 *   string that is only one (`{ ok: "✓" }`). A glyph inside words is prose — "✓ created
 *   personas/" is what the core said, and `×` in a comment names a button — and is left alone;
 * - **an `<svg>` drawn by hand**, outside the files in {@link OWN_SVG} that say why theirs is
 *   not an icon;
 * - **an icon package other than Lucide**, imported or depended on.
 *
 * jsdom draws nothing, so the source is read as text, as `settings/oldFormClasses.test.ts` does.
 */

/** Glyphs that are icons when drawn alone: checks and crosses, arrows and chevrons, warnings,
 *  stars, dots, refresh and edit marks. `…` is punctuation, and is not one. */
const GLYPHS = [..."✓✔✗✘✕✖×▸▹▶►▾▿▼◂◀◄▴▲⚠⚑★☆●○◉•↻⟳↺✎✚➕➖ⓘℹ⋮☰"];

/** Files that draw an `<svg>` of their own, and why it is not a stray icon. */
const OWN_SVG: Record<string, string> = {
  "FileIcon.tsx": "the icon theme's symbols, rebuilt from checked path data",
  "ChatGauge.tsx": "a gauge drawn to its numbers, not an icon",
};

/** The one icon package. */
const ICON_SET = "lucide-react";

const SRC = join(process.cwd(), "src");

/** Every source file the window is drawn from: not tests, which name the glyphs to deny them. */
function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return sources(path);
    return /\.tsx?$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name) ? [path] : [];
  });
}

/** `source` without its comments, so a glyph named in one is not drawn. */
const uncommented = (source: string) =>
  source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:"'`])\/\/.*$/gm, "$1");

const isGlyph = (text: string) => GLYPHS.includes(text.trim());

/** Each glyph `source` draws as an icon: alone as a JSX text, or alone in a string. */
function strayGlyphs(source: string): string[] {
  const code = uncommented(source);
  const texts = [...code.matchAll(/>([^<>{}]*)</g)].map(([, text]) => text);
  const strings = [...code.matchAll(/"([^"\n]*)"|'([^'\n]*)'|`([^`]*)`/g)].map(
    ([, a, b, c]) => a ?? b ?? c ?? "",
  );
  return [...texts, ...strings]
    .filter((text) => text.trim() !== "" && isGlyph(text))
    .map((one) => one.trim());
}

/** Whether `source` draws an `<svg>` element of its own. */
const drawsSvg = (source: string) => /<svg[\s>]/.test(uncommented(source));

/** The packages `source` imports that are an icon set and not Lucide. */
function otherIconSets(source: string): string[] {
  return [...uncommented(source).matchAll(/(?:from|import)\s*\(?\s*["']([^"'.][^"']*)["']/g)]
    .map(([, name]) => name)
    .filter((name) => /icon/i.test(name) && name !== ICON_SET);
}

describe("a stray icon", () => {
  const files = sources(SRC);
  const read = (path: string) => readFileSync(path, "utf8");

  it("is no glyph drawn as an icon", () => {
    // A reader that found nothing would pass on nothing.
    expect(files.length).toBeGreaterThan(100);
    const found = files.flatMap((path) =>
      strayGlyphs(read(path)).map((glyph) => `${relative(SRC, path)}: ${glyph}`),
    );
    expect(found).toEqual([]);
  });

  it("is no svg drawn by hand", () => {
    const found = files
      .filter((path) => drawsSvg(read(path)))
      .map((path) => relative(SRC, path))
      .filter((path) => !(path in OWN_SVG));
    expect(found).toEqual([]);
    // An entry for a file that no longer draws one is an exception nobody needs.
    for (const path of Object.keys(OWN_SVG)) expect(drawsSvg(read(join(SRC, path)))).toBe(true);
  });

  it("is from no icon package but Lucide", () => {
    const found = files.flatMap((path) =>
      otherIconSets(read(path)).map((name) => `${relative(SRC, path)}: ${name}`),
    );
    expect(found).toEqual([]);
    const pkg = JSON.parse(readFileSync(join(process.cwd(), "package.json"), "utf8")) as {
      dependencies?: Record<string, string>;
      devDependencies?: Record<string, string>;
    };
    const named = Object.keys({ ...pkg.dependencies, ...pkg.devDependencies });
    expect(named).toContain(ICON_SET);
    expect(named.filter((name) => /icon/i.test(name) && name !== ICON_SET)).toEqual([]);
  });

  it("would be caught if one came back", () => {
    // The readers above are what the guard rests on, so they are held to the shapes a screen
    // writes.
    expect(strayGlyphs(`<span className="mark">✓</span>`)).toEqual(["✓"]);
    expect(strayGlyphs(`<span aria-hidden="true">\n  ▸\n</span>`)).toEqual(["▸"]);
    expect(strayGlyphs(`const MARK = { ok: "✓", fail: '✗' };`)).toEqual(["✓", "✗"]);
    expect(strayGlyphs("<b>{`★`}</b>")).toEqual(["★"]);
    // Prose, comments and punctuation are not icons.
    expect(strayGlyphs(`said: ["✓ created personas/"]`)).toEqual([]);
    expect(strayGlyphs(`// the tab's \`×\` closes it\n/* a ▸ */`)).toEqual([]);
    expect(strayGlyphs(`<span className="pending">…</span>`)).toEqual([]);
    expect(drawsSvg(`return <svg viewBox="0 0 1 1" />;`)).toBe(true);
    expect(drawsSvg(`// an <svg> in a comment`)).toBe(false);
    expect(otherIconSets(`import { Check } from "react-icons/fa";`)).toEqual(["react-icons/fa"]);
    expect(otherIconSets(`import { Plus } from "lucide-react";`)).toEqual([]);
    expect(otherIconSets(`import { toneOf } from "./theme/icons";`)).toEqual([]);
  });
});
