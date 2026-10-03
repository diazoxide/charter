// Folds the changelog fragments in `changes/` into CHANGELOG.md's `## [Unreleased]`, then
// deletes them.
//
// A pull request adds `changes/<slug>.md` instead of editing CHANGELOG.md, so two open pull
// requests never conflict on the same lines. A fragment holds one or more Keep a Changelog
// sections (`### Added`, `### Fixed`, …), each with its entry lines, written exactly as they
// would read in CHANGELOG.md. `changes/README.md` says how to write one.
//
//   node tools/changelog-fold.mjs          fold every fragment and delete it (release prep)
//   node tools/changelog-fold.mjs --check  exit 1 when a fragment is waiting to be folded
//   node tools/changelog-fold.mjs --help   say this; any other argument is refused (exit 2)
//
// Each fragment's entries go FIRST under their heading, newest on top as the file already
// reads, and fragments are taken in name order. A heading `## [Unreleased]` does not have yet is
// made, in Keep a Changelog's order. Nothing already in the file moves.
import { readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

/** Keep a Changelog's section headings, in the order a version lists them. */
export const HEADINGS = ["Added", "Changed", "Deprecated", "Removed", "Fixed", "Security"];

/** The directory fragments live in, and the one file in it that is not a fragment. */
export const FRAGMENTS_DIR = "changes";
const NOT_A_FRAGMENT = "README.md";

/**
 * A fragment's sections, as `[heading, entry]` pairs in the order it gives them. Refuses a
 * fragment a reader of CHANGELOG.md could not place: text before the first heading, a heading
 * Keep a Changelog does not have, a heading of another level, or a heading with nothing under it.
 */
export function sections({ name, text }) {
  const out = [];
  let current = null;
  for (const line of text.replace(/\r\n/g, "\n").split("\n")) {
    const heading = /^###\s+(.+?)\s*$/.exec(line);
    if (heading) {
      if (!HEADINGS.includes(heading[1])) {
        throw new Error(
          `${name}: "### ${heading[1]}" is not a changelog heading; use one of ${HEADINGS.join(", ")}`,
        );
      }
      current = { heading: heading[1], lines: [] };
      out.push(current);
    } else if (/^#{1,6}\s/.test(line)) {
      throw new Error(`${name}: "${line}" is not a "### <heading>" line`);
    } else if (current) {
      current.lines.push(line);
    } else if (line.trim() !== "") {
      throw new Error(`${name}: the entry comes before any "### <heading>" line`);
    }
  }
  if (out.length === 0) throw new Error(`${name}: no "### <heading>" line, so nothing to fold`);
  return out.map(({ heading, lines }) => {
    const entry = lines.join("\n").trim();
    if (entry === "") throw new Error(`${name}: "### ${heading}" has no entry under it`);
    return [heading, entry];
  });
}

/** `changelog` with every fragment's entries under `## [Unreleased]`. */
export function fold(changelog, fragments) {
  const byHeading = new Map();
  for (const fragment of [...fragments].sort((a, b) => a.name.localeCompare(b.name))) {
    for (const [heading, entry] of sections(fragment)) {
      byHeading.set(heading, [...(byHeading.get(heading) ?? []), entry]);
    }
  }
  const lines = changelog.split("\n");
  for (const heading of HEADINGS) {
    const entries = byHeading.get(heading);
    if (entries) insert(lines, heading, entries.join("\n\n").split("\n"));
  }
  return lines.join("\n");
}

/** Puts `block` first under `### heading` in `## [Unreleased]`, making the heading if need be. */
function insert(lines, heading, block) {
  const start = lines.findIndex((line) => /^## \[Unreleased\]/.test(line));
  if (start === -1) throw new Error("CHANGELOG.md has no `## [Unreleased]` section to fold into");
  let end = lines.findIndex((line, i) => i > start && (/^## /.test(line) || /^\[[^\]]+\]: /.test(line)));
  if (end === -1) end = lines.length;

  const at = lines.findIndex((line, i) => i > start && i < end && line.trim() === `### ${heading}`);
  if (at !== -1) {
    const after = lines[at + 1] === "" ? [] : [""];
    lines.splice(at + 1, 0, "", ...block, ...after);
    return;
  }
  // Before the first heading that comes later in Keep a Changelog's order, or at the end.
  const later = HEADINGS.slice(HEADINGS.indexOf(heading) + 1).map((h) => `### ${h}`);
  let before = lines.findIndex((line, i) => i > start && i < end && later.includes(line.trim()));
  if (before === -1) {
    before = end;
    while (before - 1 > start && lines[before - 1] === "") before--;
  }
  const lead = lines[before - 1] === "" ? [] : [""];
  const trail = lines[before] === "" ? [] : [""];
  lines.splice(before, 0, ...lead, `### ${heading}`, "", ...block, ...trail);
}

/** The fragments waiting in `root`'s `changes/`, by name. */
export function pending(root) {
  let names;
  try {
    names = readdirSync(join(root, FRAGMENTS_DIR));
  } catch (e) {
    if (e.code === "ENOENT") return [];
    throw e;
  }
  // A file here that is not a fragment would be skipped and its entry lost, so it is refused.
  const strays = names.filter((name) => name !== NOT_A_FRAGMENT && !/^[^.].*[^.]\.md$/.test(name));
  if (strays.length > 0)
    throw new Error(
      `changes/ holds ${strays.join(", ")}, which is not a fragment: name each one <slug>.md`,
    );
  return names.filter((name) => name !== NOT_A_FRAGMENT).sort();
}

const USAGE = `usage: node tools/changelog-fold.mjs [--check | --help]
  (no argument)  fold every fragment in changes/ into CHANGELOG.md and delete it
  --check        exit 1 when a fragment is waiting to be folded; writes nothing`;

/**
 * What the command line asks for. Anything but nothing, `--check` or `--help` is a usage error,
 * never a fold: a guessed flag must not rewrite CHANGELOG.md and delete the fragments (#1023).
 */
export function mode(args) {
  if (args.length === 0) return { run: "fold" };
  if (args.length === 1 && (args[0] === "--help" || args[0] === "-h")) return { run: "help" };
  if (args.length === 1 && args[0] === "--check") return { run: "check" };
  const bad = args.find((arg) => arg !== "--check") ?? args[1];
  return { run: "usage", bad };
}

/**
 * Folds `root`'s fragments into its CHANGELOG.md and deletes them. Writes nothing when a
 * fragment is refused. Returns the fragments' names.
 */
export function foldInto(root) {
  const names = pending(root);
  const path = join(root, "CHANGELOG.md");
  const fragments = names.map((name) => ({
    name,
    text: readFileSync(join(root, FRAGMENTS_DIR, name), "utf8"),
  }));
  writeFileSync(path, fold(readFileSync(path, "utf8"), fragments));
  for (const name of names) rmSync(join(root, FRAGMENTS_DIR, name));
  return names;
}

/** Whether this file is the script node was asked to run, however the path to it was spelled. */
function runAsScript() {
  try {
    return realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url));
  } catch {
    return false;
  }
}

if (runAsScript()) {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const asked = mode(process.argv.slice(2));
  if (asked.run === "help") {
    console.log(USAGE);
    process.exit(0);
  }
  if (asked.run === "usage") {
    console.error(`unknown argument ${asked.bad}\n${USAGE}`);
    process.exit(2);
  }
  try {
    if (asked.run === "check") {
      const names = pending(root);
      if (names.length > 0) {
        console.error(
          `${names.length} changelog fragment(s) not folded into CHANGELOG.md: ${names.join(", ")}. ` +
            "Run `node tools/changelog-fold.mjs` and commit the result.",
        );
        process.exit(1);
      }
    } else {
      const names = foldInto(root);
      console.log(names.length ? `folded ${names.join(", ")}` : "no fragments to fold");
    }
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}
