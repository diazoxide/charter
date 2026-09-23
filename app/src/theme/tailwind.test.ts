/**
 * What Tailwind can and cannot say in this app.
 *
 * `styles.css` claims two things that nothing else checks, both load-bearing: that a utility
 * named after a charter token resolves to that token's custom property, and that Tailwind's own
 * 250-colour palette has been **deleted**, so `bg-red-500` is not a class — it is a typo.
 *
 * Neither claim is visible in a built stylesheet, because Tailwind only emits a utility that
 * something uses, and today nothing uses one. So this asks Tailwind itself, through its own
 * compiler, with `styles.css` as the input: the same file the app builds from, not a copy of it.
 */

/// <reference types="node" />
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { compile } from "tailwindcss";
import { describe, expect, it } from "vitest";

const SRC = join(process.cwd(), "src");

/** Compiles the app's real stylesheet and asks it for these class names. */
async function stylesheet(...candidates: string[]): Promise<string> {
  const compiled = await compile(readFileSync(join(SRC, "styles.css"), "utf8"), {
    base: SRC,
    // Tailwind resolves `@import` itself, and the app's stylesheet imports its own `App.css`
    // as well as Tailwind's parts.
    loadStylesheet: async (id: string, base: string) => {
      const path = id.startsWith(".") ? join(base, id) : join(process.cwd(), "node_modules", id);
      return { path, base: dirname(path), content: readFileSync(path, "utf8") };
    },
  });
  return compiled.build(candidates);
}

describe("Tailwind is wired to the tokens and to nothing else", () => {
  it("gives a utility named after a token that token's custom property", async () => {
    const css = await stylesheet("bg-surface-base", "text-text-muted", "border-border-subtle");
    // Both halves, because either alone passes for the wrong reason: the rule exists, and the
    // Tailwind colour it names is the charter token rather than a value of Tailwind's own.
    // `var(--surface-base)` on its own would be found in the imported `App.css`.
    expect(css).toMatch(
      /\.bg-surface-base\s*\{[^}]*background-color:\s*var\(--color-surface-base\)/,
    );
    expect(css).toMatch(/--color-surface-base:\s*var\(--surface-base\)/);
    expect(css).toMatch(/\.text-text-muted\s*\{[^}]*color:\s*var\(--color-text-muted\)/);
    expect(css).toMatch(/--color-text-muted:\s*var\(--text-muted\)/);
    expect(css).toMatch(/--color-border-subtle:\s*var\(--border-subtle\)/);
  });

  it("has no palette at all, so a class naming a colour is not a class", async () => {
    // `--color-*: initial` in `styles.css` is what does this. It is the "semantic, not
    // palette" rule enforced by the build rather than by whoever reviews the diff.
    const css = await stylesheet("bg-red-500", "text-slate-300", "border-zinc-800", "bg-white");
    expect(css).not.toContain("red-500");
    expect(css).not.toContain("slate-300");
    expect(css).not.toContain("zinc-800");
    expect(css).not.toContain(".bg-white");
  });

  it("still has the two that are not colours", async () => {
    // `transparent` is the absence of a colour and `current` is the inherited one. Deleting
    // them with the palette would make a theme unable to say "none".
    const css = await stylesheet("bg-transparent", "text-current");
    expect(css).toContain("transparent");
    expect(css).toContain("currentcolor");
  });

  it("does not bring Tailwind's reset in", async () => {
    // Preflight would restyle 1,330 lines of `App.css` in one commit, and the scenario tests
    // read the real DOM. Whoever turns it on does it as its own change, with its own evidence.
    const css = await stylesheet();
    expect(css).not.toContain("font-feature-settings");
    expect(css).not.toMatch(/\bblockquote\b/);
  });

  it("gives a motion utility the motion token, and nothing of Tailwind's own", async () => {
    // M7.2: the same bridge for timing. `ease-enter` and `duration-enter` are the theme's; a
    // bare `transition` takes `duration.quick` rather than Tailwind's 150ms; and Tailwind's own
    // easings and animations are gone the way its palette is.
    const css = await stylesheet("ease-enter", "duration-enter", "transition");
    expect(css).toMatch(/--ease-enter:\s*var\(--motion-easing-enter\)/);
    expect(css).toMatch(
      /\.duration-enter\s*\{[^}]*transition-duration:\s*var\(--transition-duration-enter\)/,
    );
    expect(css).toMatch(/--transition-duration-enter:\s*var\(--motion-duration-enter\)/);
    expect(css).toMatch(/--default-transition-duration:\s*var\(--motion-duration-quick\)/);
    expect(css).toMatch(/--default-transition-timing-function:\s*var\(--motion-easing-standard\)/);
  });

  it("has no easing or animation of Tailwind's own", async () => {
    const css = await stylesheet("ease-in-out", "ease-out", "animate-spin", "animate-pulse");
    expect(css).not.toContain(".ease-in-out");
    expect(css).not.toContain(".ease-out");
    expect(css).not.toContain(".animate-spin");
    expect(css).not.toContain(".animate-pulse");
  });

  it("puts the window's own stylesheet under the utilities", async () => {
    // Unlayered CSS beats layered CSS, so `App.css` had to be imported INTO a layer or no
    // utility could ever override a rule in it without `!important`.
    const css = await stylesheet();
    expect(css).toContain("@layer theme, base, vendor, charter, components, utilities;");
    expect(css).toMatch(/@layer charter\s*\{/);
  });

  it("puts xterm's stylesheet in a layer below charter's, so a token can beat its black", async () => {
    // M6.8. Imported from `SessionPane.tsx` it landed outside every layer and outranked the
    // whole stack; its `.xterm-viewport { background-color: #000 }` was the black band under
    // every terminal (charter-app#193). Inside `vendor` it is the library's default, and the
    // rule in `App.css` that paints the strip `--terminal-background` wins by layer order.
    const css = await stylesheet();
    const vendor = /@layer vendor\s*\{([\s\S]*?)\n\}/.exec(css)?.[1] ?? "";
    expect(vendor).toContain(".xterm .xterm-viewport");
    const charter = /@layer charter\s*\{([\s\S]*)/.exec(css)?.[1] ?? "";
    expect(charter).toMatch(
      /\.xterm \.xterm-viewport\s*\{\s*background-color:\s*var\(--terminal-background\)/,
    );
    expect(css.indexOf("@layer vendor")).toBeLessThan(css.indexOf("@layer charter {"));
  });
});

/**
 * **Every shadow Tailwind can make is drawn in `overlay.shadow`** (M6.8).
 *
 * Tailwind's own `shadow-md` carries `rgb(0 0 0 / 0.1)`, which made a shadow utility a colour no
 * theme could reach — the one leak in the "Tailwind owns no colour" rule. `styles.css` clears
 * each shadow namespace and puts the sizes back on the token. This compiles every shadow
 * utility Tailwind ships, by the name Tailwind ships it under, so a namespace that stops being
 * cleared — or a size added back with a colour of its own — fails here rather than on screen.
 */
describe("a shadow is a token's colour", () => {
  const SIZES = ["2xs", "xs", "sm", "md", "lg", "xl", "2xl"];
  const SHADOWS = [
    "shadow",
    "shadow-inner",
    ...SIZES.map((size) => `shadow-${size}`),
    ...["2xs", "xs", "sm"].map((size) => `inset-shadow-${size}`),
    "drop-shadow",
    ...SIZES.slice(1).map((size) => `drop-shadow-${size}`),
    ...["2xs", "xs", "sm", "md", "lg"].map((size) => `text-shadow-${size}`),
  ];
  /** The `utilities` layer of a compiled stylesheet, which is where Tailwind writes what it
   *  makes and nothing else is. */
  const utilities = (css: string) => css.slice(css.indexOf("@layer utilities {"));
  /** A colour written out: what a theme cannot reach. */
  const LITERAL = /#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\s*\(/;

  it.each(SHADOWS)("%s reads overlay.shadow and carries no colour of its own", async (name) => {
    // Only the utilities layer: xterm's own stylesheet has a `.shadow` of its own.
    const css = utilities(await stylesheet(name));
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const rule = new RegExp(`\\.${escaped}\\s*\\{([^}]*)\\}`).exec(css)?.[1];
    // A utility the namespace no longer makes is not a leak. One it makes is held to the token.
    if (rule === undefined) return;
    expect(rule, `${name} wrote a colour of its own`).not.toMatch(LITERAL);
    expect(rule).toContain("var(--overlay-shadow)");
  });

  it("still makes the everyday sizes", async () => {
    // Clearing a namespace and forgetting to put it back would pass the test above by making
    // nothing at all.
    for (const name of [
      "shadow-sm",
      "shadow-md",
      "shadow-lg",
      "drop-shadow-md",
      "text-shadow-sm",
    ]) {
      expect(utilities(await stylesheet(name)), name).toMatch(new RegExp(`\\.${name}\\s*\\{`));
    }
  });
});
