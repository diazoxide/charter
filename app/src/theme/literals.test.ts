/**
 * The guard on the whole layer: **no colour is written anywhere but a theme file.**
 *
 * Without this the token layer rots in a week, and the evidence that it does is the state this
 * repo was in before it: five custom properties, thirty hex literals scattered through
 * `App.css`, and a terminal drawn from two more written out in `SessionPane.tsx`. Every one of
 * those was added by somebody who only needed one colour, once.
 *
 * So the rule is mechanical and this test is where it is enforced. It reads the real source
 * tree — not a fixture — and it fails on a literal, on a Tailwind arbitrary value, on a
 * `var(--x)` that is not a token, and on a token nothing uses. `docs/design-system.md` is the
 * same rule in prose, for whoever hits this test and wants to know why.
 */

/// <reference types="vite/client" />
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { TOKENS, property, type Token } from "./theme";

/**
 * Every file the window is built from, as text, keyed by its path from `src`.
 *
 * **Vite's `import.meta.glob` says which files exist and Node says what is in them**, which is
 * two mechanisms for one job and is not a preference. The glob is resolved at build time
 * against this file's own directory, so a file added to `src` is covered by this guard without
 * anybody remembering to list it — that is the half worth having. The contents cannot come the
 * same way: vitest does not process CSS (`test.css` is off by default and turning it on would
 * change how every other test renders), so `?raw` hands back an empty string for exactly the
 * two files the rule is most about. An empty string passes every check below, silently.
 *
 * So each path is read off disk, and `nonEmpty` refuses a file that came back empty rather
 * than letting a hollow guard report success.
 */
const SOURCES: Record<string, string> = Object.fromEntries(
  Object.keys({
    ...import.meta.glob("../**/*.css"),
    ...import.meta.glob("../**/*.ts"),
    ...import.meta.glob("../**/*.tsx"),
  }).map((path) => [
    // The glob is written from `src/theme`, so Vite hands back `../App.css` for a file beside
    // `src` and collapses this directory's own back to `./theme.test.ts`. Both are said here
    // as the path from `app`, which is what a complaint should print and what `join` below
    // should take.
    path.startsWith("../") ? `src/${path.slice("../".length)}` : `src/theme/${path.slice(2)}`,
    "",
  ]),
);
// The page itself, which is not under `src` and is exactly where somebody fixing a flash of
// white at launch would reach for a colour. Named by hand because there is one of it.
SOURCES["index.html"] = "";
for (const path of Object.keys(SOURCES)) {
  SOURCES[path] = readFileSync(join(process.cwd(), path), "utf8");
}

/** A file that came back with nothing in it is a guard that checked nothing. */
function nonEmpty(path: string): string {
  const text = SOURCES[path];
  expect(text?.length ?? 0, `${path} read as empty`).toBeGreaterThan(0);
  return text;
}

/** The files the rule covers: everything but the theme's own, which is the exception it makes. */
function sources(...extensions: string[]): [string, string][] {
  const covered = Object.entries(SOURCES)
    .filter(([path]) => !path.startsWith("src/theme/"))
    .filter(([path]) => extensions.some((extension) => path.endsWith(extension)))
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([path]) => [path, nonEmpty(path)] as [string, string]);
  // A glob that matched nothing would make every case below pass by having no case to fail,
  // which is the one way a guard shaped like this dies quietly.
  expect(covered.length, `no ${extensions.join(" or ")} source was found`).toBeGreaterThan(0);
  return covered;
}

/** A file's text with its comments taken out, so that `charter-app#130` in a sentence is not
 *  read as the colour `#130`. That confusion is not hypothetical: `App.css` cites eleven
 *  issue numbers, and a guard that tripped on them would have been turned off on day one. */
function withoutComments(text: string, kind: "css" | "ts"): string {
  let stripped = text.replace(/\/\*[\s\S]*?\*\//g, " ").replace(/<!--[\s\S]*?-->/g, " ");
  // `[^:]` before the slashes so a `https://` in a sentence is not read as the start of one.
  if (kind === "ts") stripped = stripped.replace(/(^|[^:])\/\/[^\n]*/g, "$1 ");
  return stripped;
}

/** Where a complaint is, said the way an editor jumps to it. */
function at(path: string, text: string, index: number): string {
  return `${path}:${text.slice(0, index).split("\n").length}`;
}

/** Everything that is a colour written out by hand. Hex is matched with a boundary at each
 *  end so `#f0883e` is caught and `#1330` in `(charter-app#1330)` — already removed with the
 *  comments — could not sneak back as a prefix of a longer word. */
const LITERALS: { what: string; pattern: RegExp }[] = [
  { what: "a hex colour", pattern: /#[0-9a-fA-F]{3,8}\b/g },
  { what: "a colour function", pattern: /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\s*\(/g },
  {
    what: "a named CSS colour",
    pattern:
      /(?<![\w-])(?:white|black|red|green|blue|yellow|orange|purple|pink|brown|gray|grey|silver|gold|cyan|magenta|teal|navy|olive|maroon|lime|aqua|fuchsia)(?![\w-])/g,
  },
];

describe("no colour is written outside a theme file", () => {
  it.each(sources(".css", ".html"))("%s holds no colour literal", (path, raw) => {
    const text = withoutComments(raw, "css");
    const complaints: string[] = [];
    for (const { what, pattern } of LITERALS) {
      for (const hit of text.matchAll(pattern)) {
        complaints.push(`${at(path, text, hit.index)} has ${what}: ${hit[0]}`);
      }
    }
    expect(complaints, "put the colour in src/theme/ and use var(--<token>)").toEqual([]);
  });

  it.each(sources(".ts", ".tsx"))("%s holds no colour literal", (path, raw) => {
    const text = withoutComments(raw, "ts");
    const complaints: string[] = [];
    // In TypeScript a colour is only a colour inside a string — `#f85149` is not otherwise
    // valid code, and a bare `red` is an identifier. A quoted one is what reaches the DOM,
    // which is how the terminal's hardcoded theme got in.
    for (const hit of text.matchAll(/(["'`])(#[0-9a-fA-F]{3,8}|(?:rgba?|hsla?)\([^)]*\))\1/g)) {
      complaints.push(`${at(path, text, hit.index)} has a colour literal: ${hit[0]}`);
    }
    expect(complaints, "put the colour in src/theme/ and read it through theme.ts").toEqual([]);
  });
});

describe("a Tailwind class cannot reach past the tokens", () => {
  /** Tailwind v4's escape hatch: `text-[13px]`, `bg-[#fff]`, `w-[calc(100%-2rem)]`. Every one
   *  of them is a value a theme cannot change, which is the same defect as a hex literal with
   *  a different spelling. `(--var)` is the *other* v4 syntax and is fine: it names a custom
   *  property, so a theme still owns the value. */
  const ARBITRARY = /\b[a-z][a-z0-9-]*-\[[^\]\s]+\]/g;

  it.each(sources(".tsx", ".ts", ".css", ".html"))("%s uses no arbitrary value", (path, raw) => {
    const text = withoutComments(raw, path.endsWith(".ts") || path.endsWith(".tsx") ? "ts" : "css");
    const complaints = [...text.matchAll(ARBITRARY)].map(
      (hit) => `${at(path, text, hit.index)} has ${hit[0]}`,
    );
    expect(complaints, "add a token or a Tailwind theme value; do not inline one").toEqual([]);
  });
});

describe("the stylesheet and the vocabulary agree", () => {
  const css = nonEmpty("src/App.css");
  const bridge = nonEmpty("src/styles.css");
  const used = new Set([...css.matchAll(/var\((--[a-z0-9-]+)/g)].map((hit) => hit[1]));
  const declared = new Set(TOKENS.map(property));

  it("every custom property the stylesheet reads is a token", () => {
    // A `var(--typo)` resolves to nothing and the rule silently disappears, which is the one
    // failure mode of a token layer that no screenshot catches.
    expect([...used].filter((name) => !declared.has(name))).toEqual([]);
  });

  it("every token the window has is drawn with", () => {
    // A token nobody reads is a promise to a theme author that charter does not keep.
    const terminal = (token: Token) => token.startsWith("terminal.");
    const unread = TOKENS.filter((token) => !terminal(token) && !used.has(property(token)));
    expect(unread).toEqual([]);
  });

  it("every token Tailwind offers as a colour is a token, and every token is offered", () => {
    // The bridge is hand-written, so this is what keeps it from drifting from the vocabulary
    // in either direction — a Tailwind colour charter does not have, or a charter token no
    // utility can reach.
    const offered = new Set(
      [...bridge.matchAll(/--color-([a-z0-9-]+):\s*var\((--[a-z0-9-]+)\)/g)].map((hit) => hit[2]),
    );
    const wanted = TOKENS.filter((token) => !token.startsWith("terminal.")).map(property);
    expect([...offered].sort()).toEqual([...wanted].sort());
  });
});
