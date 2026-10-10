/// <reference types="node" />
import { readFileSync, readdirSync } from "node:fs";
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
 *
 * Two kinds of place are read beyond a named span (#1156, 2026-10-09):
 * - **A command's error.** A `#[tauri::command]` that fails answers the window with a `String`,
 *   and the window shows it word for word. So the literals of every `Err(…)`, `map_err(…)`,
 *   `ok_or(…)` and `ok_or_else(…)` inside a command, in every file of `app/src-tauri/src`, are
 *   copy. A command's file is found by its attribute, not listed, so a new one is read at once.
 * - **What an `in_window` body calls.** A helper function or a `const` in the same file is read
 *   as if its text were inline. A call this check cannot read (a helper in another module) is
 *   named in `NOT_FOLLOWED` with why it holds no copy, or the check fails.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const sources = new Map<string, string>();
/** A file's source, read once: the command places read every file of the app. */
const read = (path: string) => {
  const known = sources.get(path);
  if (known !== undefined) return known;
  const source = readFileSync(join(ROOT, path), "utf8");
  sources.set(path, source);
  return source;
};

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
type Place = {
  file: string;
  what: string;
  opener: RegExp;
  notCopy?: readonly string[];
  /** Read the opener's spans only inside these: a command's body, for its errors. */
  within?: RegExp;
  /** Also read the helpers and `const`s of the same file that a span calls or names. */
  follow?: boolean;
};

/** A panel's empty state: its headline and body. The offer is a catalogue row's id. */
const EMPTY = /\bempty: (?:panel::)?Empty \{/g;
/** A sentence the core writes for the window: the whole body of `in_window`. */
const IN_WINDOW = /\bfn in_window\([^)]*\)[^{;]*\{/g;
/** A command the window calls: its body, past the attributes and the signature. */
const COMMAND =
  /#\[tauri::command[^\]]*\](?:\s*#\[[^\]]*\])*\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+\w+[^{;]*\{/g;
/** What a command answers when it fails: the window shows this `String` as it is. */
const ERRORS = /\bErr\(|\.map_err\(|\.ok_or_else\(|\.ok_or\(/g;

/** Where the app's commands are written. */
const COMMANDS_DIR = "app/src-tauri/src";

/** Every Rust file under `dir` (relative to the repo), its folders included. */
function rustFiles(dir: string): string[] {
  return readdirSync(join(ROOT, dir), { withFileTypes: true }).flatMap((entry) => {
    const path = `${dir}/${entry.name}`;
    if (entry.isDirectory()) return rustFiles(path);
    return entry.name.endsWith(".rs") ? [path] : [];
  });
}

/** One place per file that holds a `#[tauri::command]`: the errors of each of its commands. */
const COMMAND_PLACES: readonly Place[] = rustFiles(COMMANDS_DIR)
  .filter((file) => read(file).includes("#[tauri::command"))
  .map((file) => ({ file, what: "a command's error", within: COMMAND, opener: ERRORS }));

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
  {
    file: "crates/purlis-core/src/pieces.rs",
    what: "a refused declaration",
    opener: IN_WINDOW,
    follow: true,
  },
  {
    file: "crates/purlis-core/src/worktree/mod.rs",
    what: "a refused or noted cut",
    opener: IN_WINDOW,
    follow: true,
  },
  {
    file: "crates/purlis-core/src/worktree/pointer.rs",
    what: "a folder no longer wired to its branch",
    opener: IN_WINDOW,
    follow: true,
  },
  {
    file: "crates/purlis-core/src/dispatchplace.rs",
    what: "a dispatched chat's place, refused",
    opener: IN_WINDOW,
    follow: true,
  },
];

/**
 * **Copy in Rust that breaks a rule and is kept for now**, as `path: "text"`, each with why. A
 * fault found here is named on #1156's checklist and fixed in its own change, since the Rust
 * files belong to other work; it is never added without both.
 */
const KEPT: Readonly<Record<string, string>> = {};

/**
 * **Calls an `in_window` body makes that this check does not read**, as `path::name` or `name`,
 * each with why it holds no copy. A call missing here, to a helper outside the file, fails the
 * check: read it inline, or name it here.
 */
const NOT_FOLLOWED: Readonly<Record<string, string>> = {
  "crate::shown::short": "shortens a name the person or a chat gave; it writes no words",
  "crate::dispatchunattended::SETTINGS":
    "the path to a Settings group, its labels written as they read; reached only through " +
    "`say`, for a chat nobody is at, which the window never shows",
};

/** Words that read like a call before `(` and are not one. */
const NOT_CALLS = new Set(["if", "match", "while", "for", "in", "return", "as", "move", "let"]);

/** The helpers `span` calls and the `const`s it names, by how the source spells each. */
function callsIn(masked: string, [from, to]: [number, number]): string[] {
  const span = masked.slice(from, to);
  const bare = [...span.matchAll(/(?<![\w.:!])((?:\w+::)*)([a-z_]\w*)\s*\(/g)]
    .filter((one) => !NOT_CALLS.has(one[2]))
    .map((one) => `${one[1]}${one[2]}`);
  const methods = [...span.matchAll(/\bself\.([a-z_]\w*)\s*\(/g)].map((one) => `self.${one[1]}`);
  const consts = [...span.matchAll(/(?<![\w:])((?:\w+::)*)([A-Z][A-Z0-9_]*[A-Z0-9])\b/g)].map(
    (one) => `${one[1]}${one[2]}`,
  );
  return [...new Set([...bare, ...methods, ...consts])];
}

/**
 * What `self.to_string()` says, for the type whose `impl` holds `at`: its `Display`'s `fmt`,
 * and the `#[error(…)]` text of each of its variants (thiserror's `Display`). Every variant's
 * text is read, though an arm may reach only some: one rule for all, and none is missed.
 */
function displayOf(masked: string, at: number): [number, number][] {
  const impls = [...masked.slice(0, at).matchAll(/\bimpl(?:<[^>]*>)?\s+(\w+)(?:<[^>]*>)?\s*\{/g)];
  const type = impls.at(-1)?.[1];
  if (type === undefined) return [];
  const display = spans(masked, new RegExp(`\\bDisplay for ${type}\\b[^{;]*\\{`, "g"));
  const defined = spans(masked, new RegExp(`\\benum ${type}\\b[^{;]*\\{`, "g"));
  const errors = spans(masked, /#\[error\(/g).filter(([open]) =>
    defined.some(([from, to]) => open > from && open < to),
  );
  return [...display, ...errors];
}

/**
 * Where a call or a name is written in the same file: each `fn name`'s body, a `const`'s value
 * up to its `;`, or, for `self.to_string()`, the type's `Display` (`displayOf`). Empty when the
 * file does not write it.
 */
function writtenAt(masked: string, call: string, at: number): [number, number][] {
  if (call === "self.to_string") return displayOf(masked, at);
  const name = call.replace(/^(?:Self::|self\.)/, "");
  if (name.includes("::")) return [];
  if (/^[A-Z]/.test(name)) {
    return [...masked.matchAll(new RegExp(`\\bconst ${name}\\b`, "g"))].map((one) => {
      const at = one.index ?? 0;
      const end = masked.indexOf(";", at);
      return [at, end < 0 ? masked.length : end];
    });
  }
  return spans(masked, new RegExp(`\\bfn ${name}\\b[^{;]*\\{`, "g"));
}

/**
 * The spans `place` reads in `masked`, and the calls it could not follow: its opener's spans
 * (inside `within`'s, if it names one), then, if it follows, every helper and `const` of the
 * same file they reach.
 */
function spansOf(masked: string, place: Place): { read: [number, number][]; unread: string[] } {
  const outer = place.within ? spans(masked, place.within) : null;
  const read = spans(masked, place.opener).filter(
    ([open]) => outer === null || outer.some(([from, to]) => open > from && open < to),
  );
  const unread = new Set<string>();
  if (place.follow) {
    const seen = new Set<string>();
    for (let next = 0; next < read.length; next += 1) {
      for (const call of callsIn(masked, read[next])) {
        if (seen.has(call)) continue;
        seen.add(call);
        const at = writtenAt(masked, call, read[next][0]);
        if (at.length === 0) unread.add(call);
        read.push(...at);
      }
    }
  }
  return { read, unread: [...unread] };
}

/** The strings `place` holds in `source`, each with its line, each once. */
function shownIn(source: string, place: Place): Literal[] {
  const { literals, masked } = rustLiterals(source);
  const { read } = spansOf(masked, place);
  return literals
    .filter((one) => read.some(([from, to]) => one.start > from && one.start < to))
    .filter((one) => {
      const [from] = read.find(([from, to]) => one.start > from && one.start < to) ?? [0];
      return !place.notCopy?.includes(fieldOf(masked, from, one.start) ?? "");
    })
    .map((one) => ({ ...one, text: placeheld(one.text) }));
}

/** The calls `place`'s spans make in `source` that the check cannot read. */
function unreadIn(source: string, place: Place): string[] {
  return spansOf(rustLiterals(source).masked, place).unread;
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

/** What breaks the guide's rules in `place`, as `file:line "text": fault`, but what is kept. */
function faultsOf(place: Place): string[] {
  return shownIn(read(place.file), place).flatMap(({ text, line }) =>
    KEPT[`${place.file}: ${JSON.stringify(text)}`] !== undefined
      ? []
      : copyFaults(text, "shown").map(
          (fault) => `${place.file}:${line} ${JSON.stringify(text)}: ${fault}`,
        ),
  );
}

describe("the errors a command answers the window with", () => {
  const place = { file: "x.rs", what: "a test", within: COMMAND, opener: ERRORS };
  const texts = (source: string) => shownIn(source, place).map((one) => one.text);

  it("reads Err, map_err, ok_or and ok_or_else inside a command, and nothing else", () => {
    const source = `
      #[tauri::command]
      #[specta::specta]
      pub async fn open(app: AppHandle, name: String) -> Result<Vec<u8>, String> {
          let label = "not an error";
          let at = find(&name).ok_or_else(|| format!("{name} is not open"))?;
          let one = first(at).ok_or("Nothing is there".to_string())?;
          if one.is_empty() { return Err("purlis did not read it.".into()); }
          read(one).await.map_err(|err| format!("reading {name} did not finish: {err}"))
      }
      fn helper() -> Result<(), String> { Err("not a command's".into()) }`;
    expect(texts(source)).toEqual([
      "… is not open",
      "Nothing is there",
      "purlis did not read it.",
      "reading … did not finish: …",
    ]);
  });

  it("holds a bad sentence added to a command's error to the rules", () => {
    const source = `
      #[tauri::command]
      fn stop() -> Result<(), String> { Err("Something went wrong!".to_string()) }`;
    const faults = texts(source).flatMap((text) => copyFaults(text, "shown"));
    expect(faults).toEqual([
      "a stock phrase: say what happened instead",
      "an exclamation mark: say it plainly",
    ]);
  });

  it("finds every file of the app that holds a command", () => {
    const files = COMMAND_PLACES.map((one) => one.file);
    expect(files).toContain(`${COMMANDS_DIR}/opener.rs`);
    expect(files).toContain(`${COMMANDS_DIR}/lib.rs`);
    expect(files.every((file) => read(file).includes("#[tauri::command"))).toBe(true);
  });
});

describe("what an in_window body calls", () => {
  const place = { file: "x.rs", what: "a test", opener: IN_WINDOW, follow: true };
  const texts = (source: string) => shownIn(source, place).map((one) => one.text);

  it("reads a helper, a const and a method of the same file as if inline", () => {
    const source = `
      const GONE: &str = "It is gone.";
      fn listed(items: &[String]) -> String { format!("{} and more", items.len()) }
      impl X {
          pub fn in_window(&self) -> String {
              match self { Self::A(v) => listed(v), Self::B => GONE.into(), Self::C => self.said() }
          }
          fn said(&self) -> String { "Said once.".into() }
      }
      fn unread() -> &'static str { "Not reached!" }`;
    expect(texts(source)).toEqual(["It is gone.", "… and more", "Said once."]);
    expect(unreadIn(source, place)).toEqual([]);
  });

  it("reads self.to_string() as the type's Display and its variants' error text", () => {
    const source = `
      #[derive(thiserror::Error)]
      pub enum X {
          #[error("'{0}' is NEVER a repo")]
          Bad(String),
      }
      impl X {
          pub fn in_window(&self) -> String { self.to_string() }
      }`;
    expect(texts(source)).toEqual(["'…' is NEVER a repo"]);
    // And the rules see the fault in it.
    expect(copyFaults(texts(source)[0], "shown")).toHaveLength(1);
  });

  it("names a call into another module it cannot read", () => {
    const source = `
      impl X {
          pub fn in_window(&self) -> String { crate::other::sentence(&self.0) + ELSEWHERE }
      }`;
    expect(unreadIn(source, place)).toEqual(["crate::other::sentence", "ELSEWHERE"]);
  });
});

describe("the window's copy written in Rust", () => {
  for (const place of PLACES) {
    it(`follows the copy guide's rules: ${place.what} (${place.file})`, () => {
      // A place that reads nothing would pass by having nothing to fail.
      expect(
        shownIn(read(place.file), place).length,
        `${place.file} still has ${place.what}`,
      ).toBeGreaterThan(0);
      expect(faultsOf(place)).toEqual([]);
    });
  }

  it("follows the copy guide's rules: every command's error, in every file of the app", () => {
    // Most commands pass an error on as it was said, so a file may read nothing; the whole
    // reading may not, and it finds the error a known command writes.
    const found = COMMAND_PLACES.flatMap((place) =>
      shownIn(read(place.file), place).map((one) => `${place.file}: ${one.text}`),
    );
    expect(COMMAND_PLACES.length).toBeGreaterThan(20);
    expect(found).toContain(
      `${COMMANDS_DIR}/opener.rs: reading what this launch would reopen did not finish: …`,
    );
    expect(COMMAND_PLACES.flatMap(faultsOf)).toEqual([]);
  });

  it("reads every call an in_window body makes, or names why it need not", () => {
    const unread = PLACES.filter((place) => place.follow).flatMap((place) =>
      unreadIn(read(place.file), place)
        .filter((call) => NOT_FOLLOWED[call] === undefined)
        .map((call) => `${place.file}: ${call}`),
    );
    expect(unread).toEqual([]);
  });

  it("keeps only faults that are still there, and only calls still made", () => {
    const all = [...PLACES, ...COMMAND_PLACES].flatMap((place) =>
      shownIn(read(place.file), place).map((one) => `${place.file}: ${JSON.stringify(one.text)}`),
    );
    expect(Object.keys(KEPT).filter((kept) => !all.includes(kept))).toEqual([]);
    const calls = PLACES.filter((place) => place.follow).flatMap((place) =>
      unreadIn(read(place.file), place),
    );
    expect(Object.keys(NOT_FOLLOWED).filter((call) => !calls.includes(call))).toEqual([]);
  });
});
