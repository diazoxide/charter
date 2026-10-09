/// <reference types="node" />
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { copyFaults } from "./copy";

/**
 * **The window's copy written in Rust is held to the same rules** (#1156, DS-2).
 *
 * `copy.test.ts` reads every string in `app/src`, but some of what the window shows is written
 * in Rust and arrives finished: a panel's empty state, the native menu's labels, and the
 * sentences the core writes for the window (`in_window`). This reads those from their source,
 * as `settings/linkedGroups.test.ts` reads the core's group ids, and holds each string literal
 * to `copyFaults(…, "shown")`. It builds nothing.
 *
 * **Which strings are shown is named here, by place**, not guessed: a Rust file mixes copy with
 * ids, paths and `Display` text for the terminal, and only the places below are the window's.
 * A place that matches nothing fails, so a rename cannot drop a file out of the check.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const read = (path: string) => readFileSync(join(ROOT, path), "utf8");

/** A string literal in a Rust source: its value as Rust reads it, and where it starts. */
type Literal = { text: string; start: number; line: number };

/** A format string's `{name}` is read as `…`, as a TypeScript template's `${…}` is; `{{` is `{`. */
function placeheld(text: string): string {
  return text.replace(/\{\{|\}\}|\{[^{}]*\}/g, (one) =>
    one === "{{" ? "{" : one === "}}" ? "}" : "…",
  );
}

/** The value of a plain literal's body: escapes read, and a `\` at a line's end joining lines. */
function unescaped(body: string): string {
  return body.replace(/\\(?:\r?\n\s*|u\{([0-9a-fA-F]+)\}|x([0-9a-fA-F]{2})|(.))/g, (_, u, x, c) => {
    if (u) return String.fromCodePoint(parseInt(u, 16));
    if (x) return String.fromCharCode(parseInt(x, 16));
    if (c === undefined) return "";
    return ({ n: "\n", t: "\t", r: "\r", "0": "\0" } as Record<string, string>)[c] ?? c;
  });
}

/**
 * Every string literal in `source`, and the source with comments and literals blanked out (the
 * same length, so a position in one is a position in the other) for finding the places in.
 *
 * Reads `"…"` with its escapes, raw `r#"…"#` and byte `b"…"` strings, and skips `//` and nested
 * `/* … *\/` comments, char literals (`'"'`) and lifetimes (`'a`), so a quote in none of them
 * opens a string.
 */
function rustLiterals(source: string): { literals: Literal[]; masked: string } {
  const literals: Literal[] = [];
  const masked = source.split("");
  const blank = (from: number, to: number) => {
    for (let at = from; at < to; at += 1) if (masked[at] !== "\n") masked[at] = " ";
  };
  const lineAt = (at: number) => source.slice(0, at).split("\n").length;
  let at = 0;
  while (at < source.length) {
    const rest = source.slice(at);
    if (rest.startsWith("//")) {
      const end = source.indexOf("\n", at);
      const to = end < 0 ? source.length : end;
      blank(at, to);
      at = to;
      continue;
    }
    if (rest.startsWith("/*")) {
      let depth = 0;
      let to = at;
      while (to < source.length) {
        if (source.startsWith("/*", to)) {
          depth += 1;
          to += 2;
        } else if (source.startsWith("*/", to)) {
          depth -= 1;
          to += 2;
          if (depth === 0) break;
        } else to += 1;
      }
      blank(at, to);
      at = to;
      continue;
    }
    const raw = /^b?r(#*)"/.exec(rest);
    if (raw && !/[A-Za-z0-9_]/.test(source[at - 1] ?? "")) {
      const close = `"${raw[1]}`;
      const end = source.indexOf(close, at + raw[0].length);
      const to = end < 0 ? source.length : end + close.length;
      literals.push({
        text: source.slice(at + raw[0].length, end < 0 ? source.length : end),
        start: at,
        line: lineAt(at),
      });
      blank(at, to);
      at = to;
      continue;
    }
    const plain = /^b?"((?:[^"\\]|\\[\s\S])*)"/.exec(rest);
    if (plain && (rest[0] === '"' || !/[A-Za-z0-9_]/.test(source[at - 1] ?? ""))) {
      literals.push({ text: unescaped(plain[1]), start: at, line: lineAt(at) });
      blank(at, at + plain[0].length);
      at += plain[0].length;
      continue;
    }
    const char = /^b?'(?:[^'\\\n]|\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.))'/.exec(rest);
    if (char && (rest[0] === "'" || !/[A-Za-z0-9_]/.test(source[at - 1] ?? ""))) {
      blank(at, at + char[0].length);
      at += char[0].length;
      continue;
    }
    at += 1;
  }
  return { literals, masked: masked.join("") };
}

/** Where each match of `opener` (ending on its `{` or `(`) opens, to where it closes. */
function spans(masked: string, opener: RegExp): [number, number][] {
  const found: [number, number][] = [];
  for (const one of masked.matchAll(opener)) {
    const open = (one.index ?? 0) + one[0].length - 1;
    const pair = masked[open] === "{" ? ["{", "}"] : ["(", ")"];
    let depth = 0;
    for (let at = open; at < masked.length; at += 1) {
      if (masked[at] === pair[0]) depth += 1;
      if (masked[at] === pair[1]) depth -= 1;
      if (depth === 0) {
        found.push([open, at]);
        break;
      }
    }
  }
  return found;
}

/** The struct field a literal is the value of: the last `name:` before it in its place. */
function fieldOf(masked: string, from: number, at: number): string | null {
  const before = [...masked.slice(from, at).matchAll(/\b([a-z_]+):(?!:)/g)];
  return before.at(-1)?.[1] ?? null;
}

/**
 * A place in the Rust source whose string literals the window shows: a file, what it is, the
 * opening of each span read (a block's `{` or a call's `(`), and the fields within it that are
 * not copy.
 */
type Place = { file: string; what: string; opener: RegExp; notCopy?: readonly string[] };

/** A panel's empty state: its headline and body. The offer is a catalogue row's id. */
const EMPTY = /\bempty: (?:panel::)?Empty \{/g;
/** A sentence the core writes for the window: the whole body of `in_window`. */
const IN_WINDOW = /\bfn in_window\([^)]*\)[^{;]*\{/g;

const PLACES: readonly Place[] = [
  {
    file: "app/src-tauri/src/panels.rs",
    what: "purlis's own panels' empty states",
    opener: EMPTY,
    notCopy: ["offer"],
  },
  {
    file: "crates/purlis-core/src/harness_card.rs",
    what: "the harness card's empty state",
    opener: EMPTY,
    notCopy: ["offer"],
  },
  {
    file: "crates/purlis-core/src/change/view.rs",
    what: "the cross-repo changes view's empty states",
    opener: EMPTY,
    notCopy: ["offer"],
  },
  {
    file: "app/src-tauri/src/lifecycle.rs",
    what: "the native menu's submenu titles",
    opener: /\bfn layout\([^)]*\)[^{;]*\{/g,
  },
  {
    file: "app/src-tauri/src/lifecycle.rs",
    what: "the native menu's and the tray's item labels",
    opener: /\bMenuItem::with_id\(/g,
  },
  { file: "crates/purlis-core/src/pieces.rs", what: "a refused declaration", opener: IN_WINDOW },
  {
    file: "crates/purlis-core/src/worktree/mod.rs",
    what: "a refused or noted cut",
    opener: IN_WINDOW,
  },
  {
    file: "crates/purlis-core/src/worktree/pointer.rs",
    what: "a folder no longer wired to its branch",
    opener: IN_WINDOW,
  },
  {
    file: "crates/purlis-core/src/dispatchplace.rs",
    what: "a dispatched chat's place, refused",
    opener: IN_WINDOW,
  },
];

/**
 * **Copy in Rust that breaks a rule and is kept for now**, as `path: "text"`, each with why. A
 * fault found here is named on #1156's checklist and fixed in its own change, since the Rust
 * files belong to other work; it is never added without both.
 */
const KEPT: Readonly<Record<string, string>> = {};

/** The strings `place` holds in `source`, each with its line. */
function shownIn(source: string, place: Place): Literal[] {
  const { literals, masked } = rustLiterals(source);
  return spans(masked, place.opener).flatMap(([from, to]) =>
    literals
      .filter((one) => one.start > from && one.start < to)
      .filter((one) => !place.notCopy?.includes(fieldOf(masked, from, one.start) ?? ""))
      .map((one) => ({ ...one, text: placeheld(one.text) })),
  );
}

describe("reading a Rust source's string literals", () => {
  const texts = (source: string) => rustLiterals(source).literals.map((one) => one.text);

  it("reads escapes and joins a line ended by a backslash", () => {
    expect(texts(String.raw`let a = "say \"no\"\n";`)).toEqual(['say "no"\n']);
    expect(texts('let a = "one line \\\n     carried on";')).toEqual(["one line carried on"]);
  });

  it("reads raw and byte strings whole", () => {
    expect(texts('let a = r#"a "quoted" word"#; let b = b"bytes";')).toEqual([
      'a "quoted" word',
      "bytes",
    ]);
  });

  it("opens no string in a comment, a char literal or a lifetime", () => {
    expect(
      texts(`// "not this"
      /* nor "this" /* nested "one" */ */
      fn f<'a>(x: &'a str) -> char { let q = '"'; let e = '\\''; "this" }`),
    ).toEqual(["this"]);
  });

  it("reads a format string's placeholders as …", () => {
    expect(placeheld("'{ws}' does not name a workspace, {{ok}} {} {0:?}")).toEqual(
      "'…' does not name a workspace, {ok} … …",
    );
  });
});

describe("the places the window's Rust copy is read from", () => {
  it("reads an empty state's headline and body, and not its offer", () => {
    const source = `
      empty: panel::Empty {
          headline: "Nothing Remembered Yet".into(),
          body: Some(format!("Add {} with the box.", n)),
          offer: Some("todo.add".into()),
      },
      title: "Elsewhere".into(),`;
    const place = { file: "x.rs", what: "a test", opener: EMPTY, notCopy: ["offer"] };
    expect(shownIn(source, place).map((one) => one.text)).toEqual([
      "Nothing Remembered Yet",
      "Add … with the box.",
    ]);
    // And the rules see the fault in it.
    expect(copyFaults("Nothing Remembered Yet", "shown")).toHaveLength(1);
  });

  it("reads the whole body of in_window, past nested blocks", () => {
    const source = `
      impl X {
          pub fn in_window(&self) -> String {
              match self { Self::A => "First sentence.".into(), Self::B { b } => format!("{b} Second!") }
          }
          fn other(&self) -> &str { "not shown" }
      }`;
    const place = { file: "x.rs", what: "a test", opener: IN_WINDOW };
    expect(shownIn(source, place).map((one) => one.text)).toEqual(["First sentence.", "… Second!"]);
  });
});

describe("the window's copy written in Rust", () => {
  for (const place of PLACES) {
    it(`follows the copy guide's rules: ${place.what} (${place.file})`, () => {
      const found = shownIn(read(place.file), place);
      // A place that reads nothing would pass by having nothing to fail.
      expect(found.length, `${place.file} still has ${place.what}`).toBeGreaterThan(0);
      const faults = found.flatMap(({ text, line }) =>
        KEPT[`${place.file}: ${JSON.stringify(text)}`] !== undefined
          ? []
          : copyFaults(text, "shown").map(
              (fault) => `${place.file}:${line} ${JSON.stringify(text)}: ${fault}`,
            ),
      );
      expect(faults).toEqual([]);
    });
  }

  it("keeps only faults that are still there", () => {
    const all = PLACES.flatMap((place) =>
      shownIn(read(place.file), place).map((one) => `${place.file}: ${JSON.stringify(one.text)}`),
    );
    expect(Object.keys(KEPT).filter((kept) => !all.includes(kept))).toEqual([]);
  });
});
