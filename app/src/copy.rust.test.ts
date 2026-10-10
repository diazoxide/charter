/// <reference types="node" />
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { copyFaults, retiredTerms } from "./copy";

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
 * Three kinds of place are read beyond a named span (#1156):
 * - **A command's error.** A `#[tauri::command]` that fails answers the window with a `String`,
 *   and the window shows it word for word. So the literals of every `Err(…)`, `map_err(…)`,
 *   `ok_or(…)` and `ok_or_else(…)` inside a command, in every file of `app/src-tauri/src`, are
 *   copy. A command's file is found by its attribute, not listed, so a new one is read at once.
 *   So are those of every function of the same file a command calls, and the helpers and
 *   `const`s that build them, called or passed by name (2026-10-10).
 * - **What an `in_window` body or a command's error calls.** A helper function or a `const` of
 *   the same file is read as if its text were inline. One of another module of the app or the
 *   core is found by the call's path and the file's `use`s, and read when its words are the
 *   sentence: a `const`, or a function that returns a `String` or a `&str` (2026-10-11, the
 *   2026-10-10 comment's item 2). A call the check cannot find is named in `NOT_FOLLOWED` with
 *   why it holds no copy, or the check fails.
 * - **A core error a command passes on as it was said** (`.map_err(|e| e.to_string())`, the
 *   2026-10-10 comment's item 1). The call it comes from is found as above (a method by its
 *   name, or by `METHODS`), and its signature names the error: a type's `Display` and
 *   `#[error(…)]` text is read, every variant's, and a `String` error's own `Err(…)` literals,
 *   with what that function passes on in turn. What cannot be read (an error the operating
 *   system words, a call not found) is named in `NOT_READ`, or the check fails. A core
 *   sentence right for the terminal with a word the window retires is named, with the ticket
 *   that gives the window its own words, in `TERMINAL_WORDS`.
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

/** Where a span opens (a block's `{` or a call's `(`) and closes, in a source. */
type Span = [number, number];
/** A span of one file of the tree. */
type Reach = { file: string; span: Span };

/**
 * **The Rust sources the check reads**: the app's and the core's on disk, or a fixture's.
 * `paths` are where a call or a type is looked for, and `crates` names each crate a path may
 * start with (`purlis_core::…`) by its `src`.
 */
type Tree = {
  paths: readonly string[];
  read: (path: string) => string;
  crates: Readonly<Record<string, string>>;
};

/** A file's source with its comments and literals blanked out, read once. */
const maskedOf = (tree: Tree, file: string) => rustLiterals(tree.read(file)).masked;

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
  const known = scanned.get(source);
  if (known !== undefined) return known;
  const literals: Literal[] = [];
  const masked = source.split("");
  const blank = (from: number, to: number) => {
    for (let at = from; at < to; at += 1) if (masked[at] !== "\n") masked[at] = " ";
  };
  // Literals are met in order, so the line is counted on from the last one, not from the top.
  let counted = 0;
  let line = 1;
  const lineAt = (at: number) => {
    for (; counted < at; counted += 1) if (source[counted] === "\n") line += 1;
    return line;
  };
  // Sticky (`y`) patterns match at `lastIndex`, so no copy of the rest is made per character.
  const match = (pattern: RegExp, at: number) => {
    pattern.lastIndex = at;
    return pattern.exec(source);
  };
  let at = 0;
  while (at < source.length) {
    if (source.startsWith("//", at)) {
      const end = source.indexOf("\n", at);
      const to = end < 0 ? source.length : end;
      blank(at, to);
      at = to;
      continue;
    }
    if (source.startsWith("/*", at)) {
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
    const raw = match(/b?r(#*)"/y, at);
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
    const plain = match(/b?"((?:[^"\\]|\\[\s\S])*)"/y, at);
    if (plain && (source[at] === '"' || !/[A-Za-z0-9_]/.test(source[at - 1] ?? ""))) {
      literals.push({ text: unescaped(plain[1]), start: at, line: lineAt(at) });
      blank(at, at + plain[0].length);
      at += plain[0].length;
      continue;
    }
    const char = match(/b?'(?:[^'\\\n]|\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.))'/y, at);
    if (char && (source[at] === "'" || !/[A-Za-z0-9_]/.test(source[at - 1] ?? ""))) {
      blank(at, at + char[0].length);
      at += char[0].length;
      continue;
    }
    at += 1;
  }
  const found = { literals, masked: masked.join("") };
  scanned.set(source, found);
  return found;
}

/** Each source scanned once: the command places read every file of the app, in several cases. */
const scanned = new Map<string, { literals: Literal[]; masked: string }>();

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
  /**
   * And inside every function of the same file a `within` span calls, and every one those call
   * in turn: where a command's error is built when the command passes it on with `?`.
   */
  through?: boolean;
  /**
   * Also read the helpers and `const`s that a span calls or names: of the same file, or of
   * another module of the app or the core, found by the call's path or the file's `use`s.
   */
  follow?: boolean;
  /** And the text of each core error a `within` span passes on as it was said (`PASSED_ON`). */
  passedOn?: boolean;
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

/**
 * The errors of each command in `file`: in its body and in every function of the same file it
 * calls, and the helpers and `const`s that build them (`Err(gone(id))`, `.map_err(not_kept)`),
 * in this file or another module (`Err(crate::dispatchaway::refused(…))`, #1156 2026-10-10
 * item 2); and the text of each core error it passes on as it was said (item 1).
 */
const commandErrors = (file: string): Place => ({
  file,
  what: "a command's error",
  within: COMMAND,
  through: true,
  opener: ERRORS,
  follow: true,
  passedOn: true,
});

/** One place per file that holds a `#[tauri::command]`: the errors of each of its commands. */
const COMMAND_PLACES: readonly Place[] = rustFiles(COMMANDS_DIR)
  .filter((file) => read(file).includes("#[tauri::command"))
  .map(commandErrors);

/** Where the core is written. */
const CORE_DIR = "crates/purlis-core/src";

/** A test module's file: never where a call the window makes is written. */
const isTests = (path: string) => /(?:^|[/_])tests?\.rs$|\/tests?\//.test(path);

/** The app's and the core's sources, without their tests: where calls and types are looked for. */
const REPO: Tree = {
  paths: [...rustFiles(COMMANDS_DIR), ...rustFiles(CORE_DIR)].filter((path) => !isTests(path)),
  read,
  crates: { purlis_core: CORE_DIR },
};

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
 * The error's own text (`#[error(…)]` or `Display`), which the terminal prints: `in_window`
 * falls back to `self.to_string()` for some variants, and this check reads every variant's.
 */
const CUT_SENTENCE =
  "the cut error's terminal text (purlis wt), some of it naming git's own worktree command; " +
  "reworded with the CLI's copy, or each variant given a sentence of its own in in_window";
/** The same for a dispatched chat's place, beside the CLI's `--in worktree` word. */
const DISPATCH_PLACE =
  "the dispatch-place error's terminal text (purlis dispatch --in worktree); reworded with " +
  "the CLI's copy, or given a sentence of its own in in_window";

/**
 * **Retired terms in the window's Rust copy, kept for now** (FR-3, #602), as `path: "text"`,
 * each with why: the Rust half of `copy.test.ts`'s `RETIRED_TERM_DEBT`, exactly, so paying one
 * off or adding one is a visible change. Empty is the goal. What is left is the core's sentences
 * the terminal prints too, which move with the CLI's copy (#602).
 */
const RETIRED_TERM_DEBT: Readonly<Record<string, string>> = {
  "crates/purlis-core/src/worktree/mod.rs: \"'\u2026' does not name a workspace this plane contains\"":
    CUT_SENTENCE,
  "crates/purlis-core/src/worktree/mod.rs: \"'\u2026' does not name a piece (letters, digits, '.', '_', '-', starting with a letter or digit)\"":
    CUT_SENTENCE,
  "crates/purlis-core/src/worktree/mod.rs: \"purlis will not cut a worktree called '\u2026': \u2026. A piece's directory name is recorded in the clone's git config and reaches every machine the branch does\"":
    CUT_SENTENCE,
  'crates/purlis-core/src/worktree/mod.rs: "this plane relocates its worktree root ([plane] worktrees = \u2026), and this version of purlis does not follow that yet: unset [plane] worktrees to keep worktrees in the plane"':
    CUT_SENTENCE,
  "crates/purlis-core/src/worktree/mod.rs: \"branch '\u2026' already exists in \u2026. Pick another piece name, or remove the branch if nothing on it is needed\"":
    CUT_SENTENCE,
  "crates/purlis-core/src/worktree/mod.rs: \"no worktree '\u2026' for \u2026 in workspace '\u2026'. See what exists: git -C <clone> worktree list\"":
    CUT_SENTENCE,
  "crates/purlis-core/src/worktree/mod.rs: \"could not determine whether '\u2026' holds uncommitted changes \u2014 refusing to remove. Check the worktree by hand, or discard with --force\"":
    CUT_SENTENCE,
  "crates/purlis-core/src/worktree/mod.rs: \"could not determine whether '\u2026' holds commits that exist nowhere else \u2014 refusing to remove. Check the worktree by hand, or discard with --force\"":
    CUT_SENTENCE,
  "crates/purlis-core/src/worktree/mod.rs: \"'\u2026' was cut, but purlis could not record the branch it came from (\u2026), so a merge would not know where to land it. The worktree is there; record it by hand: git -C <clone> config --replace-all branch.\u2026.charterBase <base>\"":
    CUT_SENTENCE,
  "crates/purlis-core/src/worktree/mod.rs: \"purlis could not record the branch '\u2026' was cut from (\u2026), so it took the worktree back\u2026\"":
    CUT_SENTENCE,
  'crates/purlis-core/src/worktree/mod.rs: ", and could not (\u2026): the worktree and its branch remain"':
    CUT_SENTENCE,
  "crates/purlis-core/src/dispatchplace.rs: \"a worktree is cut from the repo the asking chat works in, and this chat works in no repo's clone: this folder is not a git repository purlis cuts worktrees of. Dispatch it from a chat that works in a repo, or leave out `--in worktree` and the new chat works in this chat's folder.\"":
    DISPATCH_PLACE,
  'crates/purlis-core/src/dispatchplace.rs: "purlis could not cut a worktree for this task: \u2026."':
    DISPATCH_PLACE,
};

/**
 * **Calls an `in_window` body or a command's error makes that this check cannot find**, as
 * `file: call` (the file the call is written in), each with why it holds no copy. A call
 * missing here fails the check: make it one the check finds, or name it here. A call into a
 * crate outside the app and the core (`std::fs::read`) is never named: it writes no words of
 * purlis's.
 */
const NOT_FOLLOWED: Readonly<Record<string, string>> = {
  "app/src-tauri/src/curation.rs: not_drawn":
    "a parameter: each caller passes its own closure, written in its own body (follow-up: " +
    "read a closure a command passes, #1156)",
  "app/src-tauri/src/dispatchaway.rs: tell":
    "the window's listener for the away refusals, set once at start; it sends data, not words",
  "app/src-tauri/src/dispatchaway.rs: with":
    "a parameter: the closure `reading` and `grounded` run over the ground they read",
  "app/src-tauri/src/dispatchaway.rs: Shown::default": "a derived Default: it writes no words",
};

/** Why a ticket is named beside the terminal's words: the core sentence moves with it. */
const CONTAIN_SENTENCE =
  "the core's refusal of a write outside a project's data folders, written for the terminal " +
  '("control plane"); the window shows it when it opens a persona\'s file. It moves with ' +
  "the CLI's copy (#602), or gets a sentence of its own in an in_window (#1156 follow-up)";

/**
 * **Core sentences a command passes on as they were said, with a word the guide retires that
 * is right for the terminal** (#1156, #602), as `path: "text"`, each with why and the ticket
 * that gives the window its own words. Empty is the goal; adding one needs both.
 */
const TERMINAL_WORDS: Readonly<Record<string, string>> = {
  "crates/purlis-core/src/contain.rs: \"'\u2026' resolves to '\u2026', outside the directories a control plane keeps its data in (persona-state, personas, workspaces). A committed symlink there redirects the \u2026, so purlis follows a link that lands inside them and refuses one that leaves\"":
    CONTAIN_SENTENCE,
};

/**
 * **A core error a command passes on as it was said that this check does not read**, as
 * `file: call` (a call it cannot find, or whose error it cannot tell) or as the error type
 * (one written outside purlis), each with why. A new one fails the check until it is read or
 * named here.
 */
const NOT_READ: Readonly<Record<string, string>> = {
  "io::Error":
    'the operating system\'s own words ("No such file or directory (os error 2)"), not ' +
    "purlis's: a command whose window needs more says so around it",
};

/**
 * **The method a command passes an error on from, where its name is not enough**: several
 * methods of that name answer a `Result`, and the receiver's type is not read. As
 * `file: .method` (the command's file), naming the file of the one it calls.
 */
const METHODS: Readonly<Record<string, string>> = {
  "app/src-tauri/src/lib.rs: .personas": "crates/purlis-core/src/workspaces.rs",
  "app/src-tauri/src/personas.rs: .personas": "crates/purlis-core/src/workspaces.rs",
  "app/src-tauri/src/piecefiles.rs: .place": "crates/purlis-core/src/files.rs",
  "app/src-tauri/src/piecefiles.rs: .open": "crates/purlis-core/src/files.rs",
};

/** Words that read like a call before `(` and are not one. */
const NOT_CALLS = new Set(["if", "match", "while", "for", "in", "return", "as", "move", "let"]);

/**
 * The helpers `span` calls and the `const`s it names, by how the source spells each. A function
 * an error is mapped through by name (`.map_err(not_kept)`, `.ok_or_else(gone)`) is a call too.
 */
function callsIn(masked: string, [from, to]: [number, number]): string[] {
  const span = masked.slice(from, to + 1);
  const bare = [...span.matchAll(/(?<![\w.:!])((?:\w+::)*)([a-z_]\w*)\s*\(/g)]
    .filter((one) => !NOT_CALLS.has(one[2]))
    .map((one) => `${one[1]}${one[2]}`);
  const passed = [...span.matchAll(/\.(?:map_err|ok_or_else)\(\s*((?:\w+::)*)([a-z_]\w*)\s*\)/g)];
  // The span may be that argument itself, when the opener is `.map_err(` (a command's error).
  const whole = /^\(\s*((?:\w+::)*)([a-z_]\w*)\s*\)$/.exec(span);
  if (whole && /\.(?:map_err|ok_or_else)$/.test(masked.slice(Math.max(0, from - 12), from))) {
    passed.push(whole);
  }
  const methods = [...span.matchAll(/\bself\.([a-z_]\w*)\s*\(/g)].map((one) => `self.${one[1]}`);
  const consts = [...span.matchAll(/(?<![\w:])((?:\w+::)*)([A-Z][A-Z0-9_]*[A-Z0-9])\b/g)].map(
    (one) => `${one[1]}${one[2]}`,
  );
  return [
    ...new Set([...bare, ...passed.map((one) => `${one[1]}${one[2]}`), ...methods, ...consts]),
  ];
}

/**
 * What `self.to_string()` says, for the type whose `impl` holds `at`: its `Display`'s `fmt`,
 * and the `#[error(…)]` text of each of its variants (thiserror's `Display`). Every variant's
 * text is read, though an arm may reach only some: one rule for all, and none is missed.
 */
function displayOf(masked: string, at: number): Span[] {
  const impls = [...masked.slice(0, at).matchAll(/\bimpl(?:<[^>]*>)?\s+(\w+)(?:<[^>]*>)?\s*\{/g)];
  const type = impls.at(-1)?.[1];
  return type === undefined ? [] : textOfType(masked, type);
}

/**
 * What a type says when it is shown: its `Display`'s `fmt`, and its `#[error(…)]` text, on the
 * type itself and on each variant. Empty when `masked` does not define it.
 */
function textOfType(masked: string, type: string): Span[] {
  const display = spans(masked, new RegExp(`\\bDisplay for ${type}\\b[^{;]*\\{`, "g"));
  const errors = spans(masked, /#\[error\(/g);
  const defined = [...masked.matchAll(new RegExp(`\\b(?:enum|struct)\\s+${type}\\b`, "g"))];
  const said = defined.flatMap((one) => {
    const at = one.index ?? 0;
    // The attributes above it: back to the item before, which ends in `;` or `}`.
    const above = Math.max(masked.lastIndexOf(";", at), masked.lastIndexOf("}", at));
    const body = spans(masked, new RegExp(`\\benum\\s+${type}\\b[^{;]*\\{`, "g")).find(
      ([open]) => open > at,
    );
    return errors.filter(
      ([open]) => (open > above && open < at) || (body && open > body[0] && open < body[1]),
    );
  });
  return [...display, ...said];
}

/**
 * Where a call or a name is written in `masked` itself: each `fn name`'s body and each closure
 * bound to it with `let`, or a `const`'s or `static`'s value up to its `;`. Empty when the file
 * does not write it.
 */
function writtenAt(masked: string, name: string): Span[] {
  if (/^[A-Z]/.test(name)) {
    return [...masked.matchAll(new RegExp(`\\b(?:const|static) ${name}\\b`, "g"))].map((one) => {
      const at = one.index ?? 0;
      const end = masked.indexOf(";", at);
      return [at, end < 0 ? masked.length : end];
    });
  }
  return [
    ...spans(masked, new RegExp(`\\bfn ${name}\\b[^{;]*\\{`, "g")),
    ...closures(masked, name),
  ];
}

/** Each `let name = |…| …;` in `masked`: a closure a body calls by name, to its statement's end. */
function closures(masked: string, name: string): Span[] {
  return [...masked.matchAll(new RegExp(`\\blet\\s+${name}\\s*=\\s*(?:move\\s*)?\\|`, "g"))].map(
    (one) => {
      const at = one.index ?? 0;
      let depth = 0;
      for (let end = at; end < masked.length; end += 1) {
        if ("([{".includes(masked[end])) depth += 1;
        if (")]}".includes(masked[end])) depth -= 1;
        if (depth < 0 || (depth === 0 && masked[end] === ";")) return [at, end];
      }
      return [at, masked.length];
    },
  );
}

/** The crate's `src` a file is in, and its module path there (`work/log.rs` is `work::log`). */
function rootOf(path: string): string {
  return /^(.*?\/src)\//.exec(path)?.[1] ?? "";
}
function modulePath(path: string): string[] {
  const root = rootOf(path);
  const mods = path
    .slice(root === "" ? 0 : root.length + 1)
    .replace(/\.rs$/, "")
    .split("/");
  if (["mod", "lib", "main"].includes(mods.at(-1) ?? "")) mods.pop();
  return mods;
}
function fileOfModule(tree: Tree, root: string, mods: readonly string[]): string | undefined {
  const name = mods.join("::");
  return tree.paths.find((path) => rootOf(path) === root && modulePath(path).join("::") === name);
}

/** What each name a file's `use`s bring in stands for, as a whole path (`a::b::{c, d as e}`). */
function importsOf(masked: string): Map<string, string[]> {
  const known = imported.get(masked);
  if (known !== undefined) return known;
  const found = new Map<string, string[]>();
  const expand = (tree: string, prefix: string[]) => {
    const brace = tree.indexOf("{");
    if (brace < 0) {
      const [path, alias] = tree.split("@");
      const segs = [...prefix, ...path.split("::").filter(Boolean)];
      if (segs.at(-1) === "self") segs.pop();
      const name = alias ?? segs.at(-1);
      if (name !== undefined && name !== "*") found.set(name, segs);
      return;
    }
    const head = tree.slice(0, brace).split("::").filter(Boolean);
    let depth = 0;
    let from = brace + 1;
    for (let at = brace + 1; at < tree.length; at += 1) {
      if (tree[at] === "{") depth += 1;
      if (tree[at] === "}" && depth > 0) depth -= 1;
      else if ((tree[at] === "," && depth === 0) || (tree[at] === "}" && depth === 0)) {
        if (at > from) expand(tree.slice(from, at), [...prefix, ...head]);
        from = at + 1;
      }
    }
  };
  for (const one of masked.matchAll(/\buse\s+([^;]+);/g)) {
    expand(one[1].replace(/\s+as\s+/g, "@").replace(/\s+/g, ""), []);
  }
  imported.set(masked, found);
  return found;
}
const imported = new Map<string, Map<string, string[]>>();

/**
 * The crate and module a path's qualifier names, seen from `from`: `crate::`, `self::`,
 * `super::`, another crate of the tree by name, a name `from` imports, or a module below it.
 */
function moduleOf(
  tree: Tree,
  from: string,
  qual: readonly string[],
  depth = 0,
): { root: string; mods: string[] } | null {
  const [first, ...rest] = qual;
  const root = rootOf(from);
  const here = modulePath(from);
  if (first === undefined || depth > 4) return null;
  if (first === "crate") return { root, mods: rest };
  if (first === "self") return { root, mods: [...here, ...rest] };
  if (first === "super") return { root, mods: [...here.slice(0, -1), ...rest] };
  if (tree.crates[first] !== undefined) return { root: tree.crates[first], mods: rest };
  // A module below this one first: a path's head is a module, and a name imported beside it
  // (`pub use search::{search}`) is a function of the same spelling, in another namespace.
  if (fileOfModule(tree, root, [...here, first])) return { root, mods: [...here, ...qual] };
  const path = importsOf(maskedOf(tree, from)).get(first);
  if (path !== undefined) return moduleOf(tree, from, [...path, ...rest], depth + 1);
  return null;
}

/** Of `found`, those outside every `impl` and `trait` block: what `module::name` calls. */
function freeOf(masked: string, found: Span[]): Span[] {
  const blocks = spans(masked, /\b(?:impl|trait)\b[^{;]*\{/g);
  return found.filter(([open]) => !blocks.some(([from, to]) => open > from && open < to));
}

/** Where `name` is written in the `impl` blocks of `type`: in `file`, else anywhere in its crate. */
function inImpls(tree: Tree, file: string, type: string, name: string): Reach[] {
  const opener = new RegExp(
    `\\bimpl(?:<[^>]*>)?\\s+(?:[\\w:]+\\s+for\\s+)?${type}\\b[^{;]*\\{`,
    "g",
  );
  const inFile = (path: string) => {
    const masked = maskedOf(tree, path);
    const impls = spans(masked, opener);
    return writtenAt(masked, name)
      .filter(([open]) => impls.some(([from, to]) => open > from && open < to))
      .map((span) => ({ file: path, span }));
  };
  const here = inFile(file);
  if (here.length > 0) return here;
  return tree.paths
    .filter((path) => rootOf(path) === rootOf(file) && tree.read(path).includes(`impl`))
    .filter((path) => tree.read(path).includes(` ${type}`) && tree.read(path).includes(name))
    .flatMap(inFile);
}

/**
 * Where a call `from` makes, or a name it uses, is written: in its own file (`name`,
 * `Self::name`, `self.name`), or in the module its path names, in this crate or another of the
 * tree (`crate::a::b`, `purlis_core::a::b`, an imported `a::b`, `Type::name`). Empty when the
 * check cannot find it.
 */
function writtenIn(tree: Tree, from: string, call: string, at: number, depth = 0): Reach[] {
  const masked = maskedOf(tree, from);
  if (call === "self.to_string") return displayOf(masked, at).map((span) => ({ file: from, span }));
  const segs = call.replace(/^self\./, "Self::").split("::");
  const name = segs.at(-1) ?? "";
  const qual = segs.slice(0, -1);
  if (qual.length === 0 || (qual.length === 1 && qual[0] === "Self")) {
    const here = writtenAt(masked, name).map((span) => ({ file: from, span }));
    if (here.length > 0 || qual.length === 1) return here;
    const path = importsOf(masked).get(name);
    return path === undefined || depth > 4
      ? []
      : writtenIn(tree, from, path.join("::"), at, depth + 1);
  }
  const base = moduleOf(tree, from, qual);
  if (base === null) {
    // `Type::name` for a type of this file, or one it imports.
    return /^[A-Z]/.test(qual[0]) && qual.length === 1 ? inImpls(tree, from, qual[0], name) : [];
  }
  const type = /^[A-Z]/.test(base.mods.at(-1) ?? "") ? base.mods.pop() : undefined;
  const file = fileOfModule(tree, base.root, base.mods);
  if (file === undefined) return [];
  if (type !== undefined) return inImpls(tree, file, type, name);
  const there = freeOf(maskedOf(tree, file), writtenAt(maskedOf(tree, file), name)).map((span) => ({
    file,
    span,
  }));
  if (there.length > 0 || depth > 4) return there;
  // A re-export: `pub use inner::name;` in the module the path names.
  const again = importsOf(maskedOf(tree, file)).get(name);
  return again === undefined ? [] : writtenIn(tree, file, again.join("::"), 0, depth + 1);
}

/** `outer`, and every function of the same file it calls, and every one those call in turn. */
function grown(masked: string, outer: Span[]): Span[] {
  const all = [...outer];
  const seen = new Set<string>();
  for (let next = 0; next < all.length; next += 1) {
    for (const call of callsIn(masked, all[next])) {
      // A function, not a `const`: its own errors are what reach the window.
      if (seen.has(call) || /^[A-Z]/.test(call) || call.includes("::")) continue;
      seen.add(call);
      all.push(...writtenAt(masked, call.replace(/^self\./, "")));
    }
  }
  return all;
}

/** The spans of `inner` that open inside one of `outer`. */
function inside(inner: Span[], outer: Span[]): Span[] {
  return inner.filter(([open]) => outer.some(([from, to]) => open > from && open < to));
}

/** What a function returns when its words are the sentence: a `String`, a `&str`, a `Cow<str>`. */
const SENTENCE =
  /^(?:String|&(?:'\w+)?str|(?:std::borrow::)?Cow<(?:'\w+,)?str>|impl(?:std::)?(?:fmt::)?Display)$/;

/** Whether `reach` is a `const`'s value or the body of a function that returns a sentence. */
function isSentence(tree: Tree, { file, span }: Reach): boolean {
  const masked = maskedOf(tree, file);
  if (!masked.startsWith("{", span[0])) return true;
  const returned = returnOf(signatureOf(masked, span[0]));
  return returned !== null && SENTENCE.test(returned);
}

/**
 * Whether `call`, made in `from`, is into a crate outside the tree (`std::fs::read`,
 * `serde_json::to_string`, `String::new`): code that writes no words of purlis's.
 */
function isForeign(tree: Tree, from: string, call: string): boolean {
  const masked = maskedOf(tree, from);
  const segs = call.replace(/^self\./, "Self::").split("::");
  const path = importsOf(masked).get(segs[0]);
  if (segs.length === 1) return path !== undefined && isForeign(tree, from, path.join("::"));
  const first = path?.[0] ?? segs[0];
  if (["crate", "self", "super", "Self"].includes(first)) return false;
  if (tree.crates[first] !== undefined) return false;
  if (fileOfModule(tree, rootOf(from), [...modulePath(from), first]) !== undefined) return false;
  return !new RegExp(`\\b(?:enum|struct|trait|type|mod)\\s+${first}\\b`).test(masked);
}

/**
 * Grows `read` by every helper and `const` its spans call or name, and names in `unread` each
 * call it cannot find. A helper of the same file is read whole, as if inline; one of another
 * file only when its words are the sentence (a `const`, or a function that returns a `String`
 * or a `&str`), since any other function there hands back a value, not words. A call into a
 * crate outside the tree writes no words of purlis's.
 */
function follow(tree: Tree, read: Reach[], unread: Set<string>) {
  const seen = new Set<string>();
  const had = new Set(read.map(({ file, span }) => `${file}@${span[0]}`));
  for (let next = 0; next < read.length; next += 1) {
    const { file, span } = read[next];
    for (const call of callsIn(maskedOf(tree, file), span)) {
      if (seen.has(`${file}\n${call}`)) continue;
      seen.add(`${file}\n${call}`);
      const at = writtenIn(tree, file, call, span[0]);
      if (at.length === 0 && !isForeign(tree, file, call)) unread.add(`${file}: ${call}`);
      for (const one of at) {
        if (had.has(`${one.file}@${one.span[0]}`)) continue;
        if (one.file !== file && !isSentence(tree, one)) continue;
        had.add(`${one.file}@${one.span[0]}`);
        read.push(one);
      }
    }
  }
}

/**
 * **A core error passed on as it was said** (#1156, 2026-10-10 item 1):
 * `.map_err(|e| e.to_string())` or `.map_err(ToString::to_string)`. The window shows the
 * error's `Display` word for word, so the type's text is the command's copy.
 */
const PASSED_ON =
  /\.map_err\((?=\s*(?:\|\s*(\w+)\s*\|\s*\1\s*\.\s*to_string\(\s*\)|(?:std::string::)?ToString::to_string)\s*\))/g;

/** What an error is passed on from: a function by its path, or a method (`.personas`). */
type Callee = { text: string; method: boolean };

/** The `(` that `close` closes, in `masked`; -1 when none does. */
function openOf(masked: string, close: number): number {
  let depth = 0;
  for (let at = close; at >= 0; at -= 1) {
    if (masked[at] === ")") depth += 1;
    if (masked[at] === "(") depth -= 1;
    if (depth === 0) return at;
  }
  return -1;
}

/**
 * The calls whose error a `.map_err` at `end` passes on, read back through what keeps the error
 * as it was: `.await`, `?` on an outer result, `.map(…)`, an earlier `.map_err(…)` of a join,
 * a block's last expression, and `spawn_blocking`'s closure; `.and_then(…)` passes on its
 * closure's error too. Null for an error from something other than a call (a variable).
 */
function calleesBefore(masked: string, end: number, depth = 0): (Callee | null)[] {
  let at = end;
  const also: (Callee | null)[] = [];
  for (let step = 0; step < 20 && depth < 4; step += 1) {
    while (at > 0 && /\s/.test(masked[at - 1])) at -= 1;
    if (masked.slice(0, at).endsWith(".await")) {
      at -= ".await".length;
      continue;
    }
    if (masked[at - 1] === "?" || masked[at - 1] === "}") {
      at -= 1;
      continue;
    }
    if (masked[at - 1] !== ")") break;
    const open = openOf(masked, at - 1);
    const named = /(\.?)((?:\w+::)*\w+)\s*$/.exec(masked.slice(Math.max(0, open - 300), open));
    if (open < 0 || named === null) break;
    const [whole, dot, text] = named;
    const name = text.split("::").at(-1) ?? text;
    if (dot === "." && ["map", "map_err", "and_then"].includes(name)) {
      if (name === "and_then") also.push(...calleesBefore(masked, at - 1, depth + 1));
      at = open - whole.length;
      continue;
    }
    if (name === "spawn_blocking") {
      at -= 1;
      continue;
    }
    return [{ text: dot === "." ? `.${name}` : text, method: dot === "." }, ...also];
  }
  return [null, ...also];
}

/** The code on the line a `.map_err` at `end` follows: how an unread one is named. */
function lineBefore(masked: string, end: number): string {
  return (masked.slice(0, end).trimEnd().split("\n").at(-1) ?? "").trim();
}

/** Whether a function's signature takes `self`, as a method does. */
const takesSelf = (sig: string) =>
  /\(\s*(?:&\s*(?:'\w+\s+)?(?:mut\s+)?)?(?:mut\s+)?self\b/.test(sig);

/** A function's signature: from its `fn` to the `{` its body opens with. */
function signatureOf(masked: string, open: number): string {
  return masked.slice(masked.lastIndexOf("fn ", open), open);
}

/** What a signature returns, without spaces: the type after `->` past its parameters. */
function returnOf(sig: string): string | null {
  const open = sig.indexOf("(");
  let depth = 0;
  for (let at = open; open >= 0 && at < sig.length; at += 1) {
    if (sig[at] === "(") depth += 1;
    if (sig[at] === ")") depth -= 1;
    if (depth === 0) {
      const after = /^\s*->([\s\S]*)$/.exec(sig.slice(at + 1));
      return after === null ? null : after[1].replace(/\bwhere\b[\s\S]*$/, "").replace(/\s+/g, "");
    }
  }
  return null;
}

/** Where `callee` is written, for an error passed on from `file`. */
function definitionsOf(tree: Tree, file: string, callee: Callee): Reach[] {
  if (!callee.method) {
    return writtenIn(tree, file, callee.text, 0).filter(({ file: at, span }) =>
      maskedOf(tree, at).startsWith("{", span[0]),
    );
  }
  // A method's receiver is not read, so it is found by its name: the one `METHODS` names, or
  // every method of that name in the tree that answers a `Result`, if they agree on its error.
  const name = callee.text.slice(1);
  const named = METHODS[`${file}: ${callee.text}`];
  const files =
    named !== undefined
      ? [named]
      : tree.paths.filter((path) => tree.read(path).includes(`fn ${name}`));
  return files.flatMap((path) => {
    const masked = maskedOf(tree, path);
    return writtenAt(masked, name)
      .filter(([open]) => takesSelf(signatureOf(masked, open)))
      .map((span) => ({ file: path, span }))
      .filter((def) => named !== undefined || errorOf(tree, def) !== null);
  });
}

/** The top-level parts of a type's generic arguments: `T, Result<A, B>` is two. */
function splitTop(text: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let from = 0;
  for (let at = 0; at < text.length; at += 1) {
    if ("<([".includes(text[at])) depth += 1;
    if (">)]".includes(text[at])) depth -= 1;
    if (text[at] === "," && depth === 0) {
      parts.push(text.slice(from, at));
      from = at + 1;
    }
  }
  return [...parts, text.slice(from)].filter((part) => part !== "");
}

/** The error type a `Result` names, read in `masked` (whose `type Result` it may use). */
function errorIn(masked: string, returned: string, depth = 0): string | null {
  const result = /^((?:\w+::)*)Result<(.*)>$/.exec(returned);
  if (result === null) return null;
  const args = splitTop(result[2]);
  if (args.length === 2) return args[1].replace(/^std::io::/, "io::");
  if (/(?:^|::)io::$/.test(result[1])) return "io::Error";
  if (result[1] === "anyhow::") return "anyhow::Error";
  const alias = /\btype\s+Result\b[^=;]*=\s*([^;]+);/.exec(masked);
  if (result[1] !== "" || alias === null || depth > 0) return null;
  return errorIn(masked, alias[1].replace(/\s+/g, ""), depth + 1);
}

/** The error type of the function whose body is `def`, as its signature names it. */
function errorOf(tree: Tree, { file, span }: Reach): string | null {
  const masked = maskedOf(tree, file);
  const returned = returnOf(signatureOf(masked, span[0]));
  return returned === null ? null : errorIn(masked, returned);
}

/** The file that defines the type `named` (`Refused`, `crate::contain::Refused`), seen from `from`. */
function typeFile(tree: Tree, from: string, named: string): string | null {
  const segs = named.replace(/<.*$/, "").split("::");
  const type = segs.at(-1) ?? "";
  const defines = (path: string) =>
    new RegExp(`\\b(?:enum|struct)\\s+${type}\\b`).test(maskedOf(tree, path));
  if (segs.length === 1 && defines(from)) return from;
  const path = segs.length === 1 ? importsOf(maskedOf(tree, from)).get(type) : segs;
  const base = path === undefined ? null : moduleOf(tree, from, path.slice(0, -1));
  const file = base === null ? undefined : fileOfModule(tree, base.root, base.mods);
  if (file !== undefined && defines(file)) return file;
  // A re-export, or a type its crate defines once: read where it is written.
  const once = tree.paths.filter((one) => rootOf(one) === rootOf(file ?? from) && defines(one));
  return once.length === 1 ? once[0] : null;
}

/** A type the check does not read by its nature: the standard library's and other crates'. */
const FOREIGN = /^(?:io::Error|anyhow::Error|std::|Box<dyn|serde_json::|toml::|tauri::)/;

/** What a check of passed-on errors read, and what it could not, by the names `NOT_READ` uses. */
type PassedOn = { read: Reach[]; notRead: string[] };

/**
 * The text of every core error the spans `outer` of `file` pass on as it was said, and of the
 * functions of the same file they call: a `String` error's own `Err(…)` literals (and what it
 * passes on in turn), or a named type's `Display` and `#[error(…)]`. What cannot be read is
 * named, as `file: call` or as the error type.
 */
function passedOn(tree: Tree, file: string, outer: Span[], seen: Set<string>): PassedOn {
  const masked = maskedOf(tree, file);
  const read: Reach[] = [];
  const notRead: string[] = [];
  for (const [open] of inside(spans(masked, PASSED_ON), grown(masked, outer))) {
    const end = open - ".map_err".length;
    for (const callee of calleesBefore(masked, end)) {
      const key = `${file}: ${callee?.text ?? lineBefore(masked, end)}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const defs = callee === null ? [] : definitionsOf(tree, file, callee);
      const errors = [...new Set(defs.map((def) => errorOf(tree, def)))];
      const error = errors.length === 1 ? errors[0] : null;
      if (error === null) {
        notRead.push(key);
      } else if (error === "String") {
        for (const def of defs) {
          const at = maskedOf(tree, def.file);
          read.push(
            ...inside(spans(at, ERRORS), grown(at, [def.span])).map((span) => ({
              file: def.file,
              span,
            })),
          );
          const deeper = passedOn(tree, def.file, [def.span], seen);
          read.push(...deeper.read);
          notRead.push(...deeper.notRead);
        }
      } else if (FOREIGN.test(error)) {
        notRead.push(error);
      } else {
        const at = typeFile(tree, defs[0].file, error);
        if (at === null) notRead.push(`${defs[0].file}: ${error}`);
        else {
          const type = error.replace(/<.*$/, "").split("::").at(-1) ?? error;
          read.push(...textOfType(maskedOf(tree, at), type).map((span) => ({ file: at, span })));
        }
      }
    }
  }
  return { read, notRead: [...new Set(notRead)] };
}

/**
 * The spans `place` reads in the tree, the calls it could not follow, and the passed-on errors
 * it could not read: its opener's spans (inside `within`'s, if it names one, and the functions
 * of the same file those call if it goes `through`); then, if it follows, every helper and
 * `const` they reach, in any file of the tree; and the errors it passes on as they were said.
 */
function spansOf(tree: Tree, place: Place): { read: Reach[]; unread: string[]; notRead: string[] } {
  const masked = maskedOf(tree, place.file);
  const within = place.within ? spans(masked, place.within) : null;
  const outer = within !== null && place.through ? grown(masked, within) : within;
  const own = spans(masked, place.opener);
  const read: Reach[] = (outer === null ? own : inside(own, outer)).map((span) => ({
    file: place.file,
    span,
  }));
  const passed =
    place.passedOn && within !== null
      ? passedOn(tree, place.file, within, new Set())
      : { read: [], notRead: [] };
  read.push(...passed.read);
  const unread = new Set<string>();
  if (place.follow) follow(tree, read, unread);
  return { read, unread: [...unread], notRead: passed.notRead };
}

/** A string the window shows: where it is written, and its text. */
type Said = Literal & { file: string };

/** The strings `place` holds in the tree, each with its file and line, each once. */
function shownOf(tree: Tree, place: Place): Said[] {
  const { read } = spansOf(tree, place);
  const files = [...new Set(read.map(({ file }) => file))];
  return files.flatMap((file) => {
    const { literals, masked } = rustLiterals(tree.read(file));
    const here = read.filter((one) => one.file === file).map(({ span }) => span);
    return literals.flatMap((one) => {
      const [from] = here.find(([from, to]) => one.start > from && one.start < to) ?? [-1];
      if (from < 0) return [];
      if (file === place.file && place.notCopy?.includes(fieldOf(masked, from, one.start) ?? "")) {
        return [];
      }
      return [{ ...one, file, text: placeheld(one.text) }];
    });
  });
}

/** A tree of one source, for the tests of the check itself. */
const oneFile = (file: string, source: string): Tree => ({
  paths: [file],
  read: () => source,
  crates: {},
});

/** The strings `place` holds in `source`, each with its line, each once. */
function shownIn(source: string, place: Place): Literal[] {
  return shownOf(oneFile(place.file, source), place);
}

/** The calls `place`'s spans make in `source` that the check cannot read. */
function unreadIn(source: string, place: Place): string[] {
  return spansOf(oneFile(place.file, source), place).unread;
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

/** Every place's strings, read once: the command places follow calls across the app and the core. */
const shownAt = new Map<Place, Said[]>();
function shown(place: Place): Said[] {
  const known = shownAt.get(place);
  if (known !== undefined) return known;
  const found = shownOf(REPO, place);
  shownAt.set(place, found);
  return found;
}

/** How the lists below name a string: where it is written, and its text. */
const keyOf = ({ file, text }: Said) => `${file}: ${JSON.stringify(text)}`;

/** What breaks the guide's rules in `place`, as `file:line "text": fault`, but what is kept. */
function faultsOf(place: Place): string[] {
  return shown(place).flatMap((said) =>
    KEPT[keyOf(said)] !== undefined
      ? []
      : copyFaults(said.text, "shown").map(
          (fault) => `${said.file}:${said.line} ${JSON.stringify(said.text)}: ${fault}`,
        ),
  );
}

describe("the errors a command answers the window with", () => {
  const place = commandErrors("x.rs");
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

  it("reads a helper of the same file that builds the error, called or passed by name", () => {
    const source = `
      const NOT_KEPT: &str = "purlis did not keep it.";
      fn gone(id: u32) -> String { format!("Chat {id} is gone.") }
      fn not_kept(err: std::io::Error) -> String { format!("{} {err}", NOT_KEPT) }
      fn no_such(name: &str) -> String { said(name) }
      fn said(name: &str) -> String { format!("{name} is not open.") }
      fn quiet() -> &'static str { "Not an error, never reached!" }
      #[tauri::command]
      fn stop(id: u32, name: String) -> Result<(), String> {
          let one = find(id).ok_or_else(|| gone(id))?;
          let two = find(id).ok_or_else(no_such_label)?;
          write(one).map_err(not_kept)?;
          if two { return Err(no_such(&name)); }
          Err(other::sentence(why))
      }`;
    expect(texts(source)).toEqual([
      "purlis did not keep it.",
      "Chat … is gone.",
      "… …",
      "… is not open.",
    ]);
  });

  it("reads the errors of a function of the same file the command calls, and of what that calls", () => {
    const source = `
      fn revoke(id: &str) -> Result<(), String> {
          let label = "not an error";
          find(id).ok_or_else(|| "That grant is no longer there.".to_owned())?;
          write(id)
      }
      fn write(id: &str) -> Result<(), String> { Err(format!("{id} was not written.")) }
      fn unused() -> Result<(), String> { Err("Not called by a command!".into()) }
      #[tauri::command]
      pub fn revoke_grant(id: String) -> Result<(), String> { revoke(&id) }`;
    expect(texts(source)).toEqual(["That grant is no longer there.", "… was not written."]);
  });

  it("holds a bad sentence a helper builds for a command's error to the rules", () => {
    // The mutation: the helper's sentence, not the command's own text, breaks the guide.
    const source = `
      fn gone(id: u32) -> String { format!("Chat {id} Is No Longer Open") }
      #[tauri::command]
      fn stop(id: u32) -> Result<(), String> { Err(gone(id)) }`;
    expect(texts(source)).toEqual(["Chat … Is No Longer Open"]);
    expect(texts(source).flatMap((text) => copyFaults(text, "shown"))).toHaveLength(1);
  });

  it("finds every file of the app that holds a command", () => {
    const files = COMMAND_PLACES.map((one) => one.file);
    // Tauri's builder lists every command in lib.rs, which holds some of its own.
    expect(files).toContain(`${COMMANDS_DIR}/lib.rs`);
    expect(files.every((file) => read(file).includes("#[tauri::command"))).toBe(true);
  });
});

/** A tree of fixture files, the core's under `core/src` and named `purlis_core`. */
const fixture = (files: Record<string, string>): Tree => ({
  paths: Object.keys(files),
  read: (path) => files[path],
  crates: { purlis_core: "core/src" },
});

describe("a core error a command passes on as it was said", () => {
  const COMMANDS = "app/src/commands.rs";
  const reading = (files: Record<string, string>) => {
    const tree = fixture(files);
    const place = commandErrors(COMMANDS);
    return {
      texts: shownOf(tree, place).map((one) => `${one.file}: ${one.text}`),
      notRead: spansOf(tree, place).notRead,
    };
  };

  it("reads the error type's #[error] text, every variant, through the call's path", () => {
    const { texts, notRead } = reading({
      [COMMANDS]: `
        #[tauri::command]
        pub fn open(root: String) -> Result<(), String> {
            purlis_core::store::open(&root).map_err(|why| why.to_string())
        }`,
      "core/src/store.rs": `
        pub fn open(root: &str) -> Result<(), Refused> { Err(Refused::Locked) }
        #[derive(Debug, thiserror::Error)]
        pub enum Refused {
            #[error("'{0}' is not a store")]
            NotOne(String),
            #[error("the store is LOCKED")]
            Locked,
        }
        fn elsewhere() -> &'static str { "never shown" }`,
    });
    expect(texts).toEqual([
      "core/src/store.rs: '…' is not a store",
      "core/src/store.rs: the store is LOCKED",
    ]);
    expect(notRead).toEqual([]);
    // And the rules see the fault in it: the mutation is the core's word, not the app's.
    const said = texts.map((one) => one.slice(one.indexOf(": ") + 2));
    expect(said.flatMap((text) => copyFaults(text, "shown"))).toHaveLength(1);
  });

  it("reads a Display impl, past an import, a join's ? and spawn_blocking's closure", () => {
    const { texts } = reading({
      [COMMANDS]: `
        use purlis_core::{other, store};
        #[tauri::command]
        pub async fn keep(root: String) -> Result<(), String> {
            tauri::async_runtime::spawn_blocking(move || store::keep(&root))
                .await
                .map_err(|err| format!("keeping did not finish: {err}"))?
                .map_err(|err| err.to_string())
        }`,
      "core/src/store.rs": `
        pub fn keep(root: &str) -> Result<(), Kept> { Ok(()) }
        pub struct Kept(String);
        impl fmt::Display for Kept {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "purlis kept nothing in {}", self.0)
            }
        }`,
    });
    expect(texts).toEqual([
      "app/src/commands.rs: keeping did not finish: …",
      "core/src/store.rs: purlis kept nothing in …",
    ]);
  });

  it("reads a String error's own sentences, and what that function passes on in turn", () => {
    const { texts } = reading({
      [COMMANDS]: `
        use purlis_core::grant;
        #[tauri::command]
        pub fn allow(host: String) -> Result<(), String> {
            grant::allow(&host).map_err(ToString::to_string)
        }`,
      "core/src/grant.rs": `
        use crate::hosts::Host;
        pub fn allow(host: &str) -> Result<(), String> {
            let label = "not an error";
            if host.is_empty() { return Err("Name a host to allow.".to_owned()); }
            Host::parse(host).map_err(|bad| bad.to_string())?;
            Ok(())
        }`,
      "core/src/hosts.rs": `
        pub struct Host;
        impl Host { pub fn parse(text: &str) -> Result<Host, Bad> { Err(Bad) } }
        #[derive(thiserror::Error)]
        #[error("that is not a host name")]
        pub struct Bad;`,
    });
    expect(texts).toEqual([
      "core/src/grant.rs: Name a host to allow.",
      "core/src/hosts.rs: that is not a host name",
    ]);
  });

  it("reads a re-exported function of a module and its namesake (`files::search`)", () => {
    const { texts } = reading({
      [COMMANDS]: `
        use purlis_core::files;
        #[tauri::command]
        pub fn find(text: String) -> Result<(), String> {
            files::search(&text).map_err(|bad| bad.to_string())?;
            Ok(())
        }`,
      "core/src/files.rs": `
        mod search;
        pub use search::{BadQuery, search};`,
      "core/src/files/search.rs": `
        pub fn search(text: &str) -> Result<(), BadQuery> { Ok(()) }
        #[derive(thiserror::Error)]
        pub enum BadQuery { #[error("the search is not a pattern: {0}")] Pattern(String) }`,
    });
    expect(texts).toEqual(["core/src/files/search.rs: the search is not a pattern: …"]);
  });

  it("reads both errors of an and_then, and a method its name finds", () => {
    const { texts, notRead } = reading({
      [COMMANDS]: `
        use purlis_core::files;
        #[tauri::command]
        pub fn opened(path: String) -> Result<(), String> {
            files::root().and_then(|root| root.open(&path)).map_err(|why| why.to_string())
        }`,
      "core/src/files.rs": `
        pub fn root() -> Result<Root, NoRoot> { Ok(Root) }
        pub struct Root;
        impl Root { pub fn open(&self, path: &str) -> Result<(), Unopened> { Ok(()) } }
        impl Other { pub fn open(&self) -> Option<u8> { None } }
        #[derive(thiserror::Error)] #[error("the branch has no folder")] pub struct NoRoot;
        #[derive(thiserror::Error)] #[error("the file did not open")] pub struct Unopened;`,
    });
    expect(texts).toEqual([
      "core/src/files.rs: the branch has no folder",
      "core/src/files.rs: the file did not open",
    ]);
    expect(notRead).toEqual([]);
  });

  it("names what it cannot read: a foreign error, an unfound call, two methods that disagree", () => {
    const { notRead } = reading({
      [COMMANDS]: `
        #[tauri::command]
        pub fn read(root: String, said: Result<(), Thing>) -> Result<(), String> {
            purlis_core::disk::read(&root).map_err(|e| e.to_string())?;
            purlis_core::disk::gone(&root).map_err(|e| e.to_string())?;
            Plane::open(&root).personas().map_err(|e| e.to_string())?;
            said.map_err(|e| e.to_string())
        }`,
      "core/src/disk.rs": `
        pub fn read(root: &str) -> io::Result<String> { Ok(String::new()) }
        impl Plane { pub fn personas(&self) -> io::Result<Vec<String>> { Ok(vec![]) } }`,
      "core/src/model.rs": `
        impl Model { pub fn personas(&self) -> Result<Vec<String>, String> { Ok(vec![]) } }`,
    });
    expect(notRead).toEqual([
      "io::Error",
      "app/src/commands.rs: purlis_core::disk::gone",
      "app/src/commands.rs: .personas",
      "app/src/commands.rs: said",
    ]);
  });
});

describe("a command's error built by a helper of another module (#1156, item 2)", () => {
  const COMMANDS = "app/src/commands.rs";
  const reading = (files: Record<string, string>) => {
    const tree = fixture(files);
    const place = commandErrors(COMMANDS);
    return {
      texts: shownOf(tree, place).map((one) => `${one.file}: ${one.text}`),
      unread: spansOf(tree, place).unread,
    };
  };

  it("reads a helper that returns the sentence, and a const, in the app or the core", () => {
    const { texts, unread } = reading({
      [COMMANDS]: `
        use purlis_core::within;
        #[tauri::command]
        pub fn start(name: String) -> Result<(), String> {
            if name.is_empty() { return Err(crate::away::refused(&name)); }
            if name.len() > 9 { return Err(within::not_there(&name)); }
            Err(purlis_core::place::CHANGED.to_owned())
        }`,
      "app/src/away.rs": `
        pub fn refused(name: &str) -> String { format!("{name} Was Refused By Policy") }`,
      "core/src/within.rs": `
        pub fn not_there(name: &str) -> String { format!("{} is not there.", short(name)) }
        fn short(name: &str) -> String { name.chars().take(9).collect() }`,
      "core/src/place.rs": `
        pub const CHANGED: &str = "The place changed since it was asked.";`,
    });
    expect(texts).toEqual([
      "app/src/away.rs: … Was Refused By Policy",
      "core/src/within.rs: … is not there.",
      "core/src/place.rs: The place changed since it was asked.",
    ]);
    expect(unread).toEqual([]);
    // The mutation: only the other module's helper breaks the guide, so only reading it finds it.
    const said = texts.map((one) => one.slice(one.indexOf(": ") + 2));
    expect(said.flatMap((text) => copyFaults(text, "shown"))).toHaveLength(1);
  });

  it("does not read a function of another module that answers a value, nor a foreign call", () => {
    const { texts, unread } = reading({
      [COMMANDS]: `
        #[tauri::command]
        pub fn shown(name: String) -> Result<(), String> {
            Err(format!("{} is gone.", crate::paths::of(&name, std::path::MAIN_SEPARATOR_STR).display()))
        }`,
      "app/src/paths.rs": `
        pub fn of(name: &str, sep: &str) -> PathBuf { PathBuf::from("NOT A SENTENCE") }`,
    });
    expect(texts).toEqual(["app/src/commands.rs: … is gone."]);
    expect(unread).toEqual([]);
  });

  it("names a call it cannot find, by the file it is written in", () => {
    const { unread } = reading({
      [COMMANDS]: `
        #[tauri::command]
        pub fn start() -> Result<(), String> { Err(crate::missing::sentence()) }`,
    });
    expect(unread).toEqual(["app/src/commands.rs: crate::missing::sentence"]);
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

  it("holds a bad sentence a helper builds for an in_window arm to the rules", () => {
    // The mutation: only the helper's text breaks the guide, so only following it finds it.
    const source = `
      fn listed(n: usize) -> String { format!("{n} Pieces Were Not Cut") }
      impl X {
          pub fn in_window(&self) -> String {
              match self { Self::A(n) => listed(*n), Self::B => "Nothing was cut.".into() }
          }
      }`;
    expect(texts(source)).toEqual(["… Pieces Were Not Cut", "Nothing was cut."]);
    expect(texts(source).flatMap((text) => copyFaults(text, "shown"))).toHaveLength(1);
  });

  it("names a call into another module it cannot read", () => {
    const source = `
      impl X {
          pub fn in_window(&self) -> String { crate::other::sentence(&self.0) + ELSEWHERE }
      }`;
    expect(unreadIn(source, place)).toEqual(["x.rs: crate::other::sentence", "x.rs: ELSEWHERE"]);
  });
});

describe("the window's copy written in Rust", () => {
  const ALL = [...PLACES, ...COMMAND_PLACES];
  const unique = (keys: string[]) => [...new Set(keys)].sort();

  for (const place of PLACES) {
    it(`follows the copy guide's rules: ${place.what} (${place.file})`, () => {
      // A place that reads nothing would pass by having nothing to fail.
      expect(shown(place).length, `${place.file} still has ${place.what}`).toBeGreaterThan(0);
      expect(faultsOf(place)).toEqual([]);
    });
  }

  it("follows the copy guide's rules: every command's error, in every file of the app", () => {
    // Most commands pass an error on as it was said, so a file may read nothing; the whole
    // reading may not. A floor, not a sentence: rewording one command's error is no reason for
    // this to fail, and a reader that lost the commands reads far below it.
    const found = COMMAND_PLACES.flatMap(shown);
    expect(COMMAND_PLACES.length).toBeGreaterThan(20);
    expect(found.length).toBeGreaterThan(40);
    expect(COMMAND_PLACES.flatMap(faultsOf)).toEqual([]);
  });

  it("reads the core's errors a command passes on as they were said", () => {
    // A floor, as above: a reader that lost the passed-on errors reads none of the core's.
    const fromCore = COMMAND_PLACES.flatMap(
      ({ file }) => passedOn(REPO, file, spans(maskedOf(REPO, file), COMMAND), new Set()).read,
    ).filter(({ file }) => file.startsWith(CORE_DIR));
    expect(new Set(fromCore.map(({ file }) => file)).size).toBeGreaterThan(5);
  });

  it("reads every call an in_window body or a command's error makes, or names why it need not", () => {
    const unread = ALL.filter((place) => place.follow).flatMap((place) =>
      spansOf(REPO, place).unread.filter((call) => NOT_FOLLOWED[call] === undefined),
    );
    expect(unique(unread)).toEqual([]);
  });

  it("reads every error a command passes on as it was said, or names why it cannot", () => {
    const notRead = COMMAND_PLACES.flatMap((place) => spansOf(REPO, place).notRead);
    expect(unique(notRead).filter((key) => NOT_READ[key] === undefined)).toEqual([]);
  });

  it("says no retired term but the ones listed as debt or as the terminal's words (#602)", () => {
    const retired = ALL.flatMap(shown)
      .filter(({ text }) => retiredTerms(text, "shown").length > 0)
      .map(keyOf);
    expect(unique(retired)).toEqual(
      unique([...Object.keys(RETIRED_TERM_DEBT), ...Object.keys(TERMINAL_WORDS)]),
    );
  });

  it("keeps only faults that are still there, and only calls and errors still made", () => {
    const all = new Set(ALL.flatMap(shown).map(keyOf));
    expect(Object.keys(KEPT).filter((kept) => !all.has(kept))).toEqual([]);
    const calls = new Set(
      ALL.filter((place) => place.follow).flatMap((place) => spansOf(REPO, place).unread),
    );
    expect(Object.keys(NOT_FOLLOWED).filter((call) => !calls.has(call))).toEqual([]);
    const notRead = new Set(COMMAND_PLACES.flatMap((place) => spansOf(REPO, place).notRead));
    expect(Object.keys(NOT_READ).filter((key) => !notRead.has(key))).toEqual([]);
    const methods = Object.entries(METHODS).filter(([site, file]) => {
      const [from, method] = site.split(": .");
      const masked = REPO.paths.includes(file) ? maskedOf(REPO, file) : "";
      const there = writtenAt(masked, method).filter(([open]) =>
        takesSelf(signatureOf(masked, open)),
      );
      return !read(from).includes(`.${method}(`) || there.length === 0;
    });
    expect(methods).toEqual([]);
  });
});
