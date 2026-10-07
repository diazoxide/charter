#!/usr/bin/env node
// Holds a blessed `behaviour.jsonl` to the rows and the fields that were meant to move.
//
// CI's `recorded` job uploads the file a bless would write when the replay is red
// (`recorded-behaviour-blessed`). Before that file is committed, this says whether it differs
// from the committed one only where it may:
//
// - the same rows, by name, in the same order: a bless adds, drops and renames none;
// - only rows that were named may differ at all;
// - and a named row may differ only in what a bless rewrites: the text of its stdout and
//   stderr, the status line's alert rows, the tree it leaves, and where its work lands
//   (`crates/purlis-cli/tests/recorded_behaviour/replay.rs`). Its command, its environment, its
//   stdin, its start, its masks and its verdict (`denies`, `allows`, `says`) are never a
//   bless's to move, so a difference there is refused.
//
//   node tools/recorded-bless-check.mjs <committed.jsonl> <blessed.jsonl> <name-or-prefix>...
//
// A name ending in `-` is a prefix (`doctor-`). With `--tree <path>` given one or more times, a
// named row's tree may differ only at those paths. Exit 0 and the rows that moved on stdout, or
// exit 1 and every difference that is not allowed on stderr.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** The parts of a row's `expect` a bless may rewrite, as paths into it. */
export const BLESSABLE = [
  ["stdout", "text"],
  ["stderr", "text"],
  ["alerts"],
  ["tree"],
  ["outside", "hit"],
];

/** The rows of a `behaviour.jsonl`, in order. */
export function rowsOf(text) {
  return text
    .split("\n")
    .filter((line) => line.length > 0)
    .map((line, i) => {
      try {
        return JSON.parse(line);
      } catch (error) {
        throw new Error(`line ${i + 1} is not one JSON object: ${error.message}`);
      }
    });
}

function named(name, names) {
  return names.some((want) => (want.endsWith("-") ? name.startsWith(want) : name === want));
}

function at(value, path) {
  return path.reduce((inner, key) => (inner == null ? undefined : inner[key]), value);
}

/** `value` with what a bless may rewrite taken out, so the rest can be compared whole. */
function withoutBlessable(row) {
  const copy = structuredClone(row);
  for (const path of BLESSABLE) {
    const parent = at(copy.expect, path.slice(0, -1));
    if (parent != null && typeof parent === "object") delete parent[path.at(-1)];
  }
  return copy;
}

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** The paths a tree delta sets or removes. */
function treePaths(tree) {
  return new Set([...Object.keys(tree?.set ?? {}), ...(tree?.remove ?? [])]);
}

/** The paths at which two tree deltas differ. */
function treeDifferences(before, after) {
  const out = [];
  for (const path of new Set([...treePaths(before), ...treePaths(after)])) {
    const was = [before?.set?.[path], (before?.remove ?? []).includes(path)];
    const is = [after?.set?.[path], (after?.remove ?? []).includes(path)];
    if (!same(was, is)) out.push(path);
  }
  return out.sort();
}

/**
 * What moved between `committed` and `blessed`, and every difference that is not allowed.
 * `names` are the rows that may move (a trailing `-` makes one a prefix); `treeOnly`, where it
 * holds any path, is the only paths a named row's tree may differ at.
 */
export function check(committed, blessed, names, treeOnly = []) {
  const problems = [];
  const moved = [];
  // Every row that differs at all, allowed or not: what a name is checked against below.
  const differed = [];
  const was = committed.map((row) => row.name);
  const is = blessed.map((row) => row.name);
  if (!same(was, is)) {
    const gone = was.filter((name) => !is.includes(name));
    const added = is.filter((name) => !was.includes(name));
    problems.push(
      `the rows are not the same rows in the same order (${was.length} committed, ${is.length} blessed` +
        (gone.length ? `; gone: ${gone.join(", ")}` : "") +
        (added.length ? `; added: ${added.join(", ")}` : "") +
        ")",
    );
    return { moved, problems };
  }
  committed.forEach((before, i) => {
    const after = blessed[i];
    if (same(before, after)) return;
    const name = before.name;
    differed.push(name);
    if (!named(name, names)) {
      problems.push(`${name}: moved, and it is not a row that was named`);
      return;
    }
    if (!same(withoutBlessable(before), withoutBlessable(after))) {
      problems.push(
        `${name}: differs outside what a bless rewrites (its command, start, masks or verdict)`,
      );
      return;
    }
    const where = BLESSABLE.filter(
      (path) => !same(at(before.expect, path), at(after.expect, path)),
    ).map((path) => path.join("."));
    if (treeOnly.length > 0) {
      const strays = treeDifferences(before.expect.tree, after.expect.tree).filter(
        (path) => !treeOnly.includes(path),
      );
      if (strays.length > 0) {
        problems.push(`${name}: its tree moved at ${strays.join(", ")}, which was not named`);
        return;
      }
    }
    moved.push({ name, where });
  });
  for (const want of names) {
    if (!differed.some((name) => named(name, [want]))) {
      problems.push(`${want}: named, and no such row moved`);
    }
  }
  return { moved, problems };
}

function main(argv) {
  const treeOnly = [];
  const rest = [];
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--tree") treeOnly.push(argv[++i]);
    else rest.push(argv[i]);
  }
  const [committedPath, blessedPath, ...names] = rest;
  if (!committedPath || !blessedPath || names.length === 0) {
    console.error(
      "usage: node tools/recorded-bless-check.mjs [--tree <path>]... <committed.jsonl> <blessed.jsonl> <name-or-prefix>...",
    );
    return 2;
  }
  const { moved, problems } = check(
    rowsOf(readFileSync(committedPath, "utf8")),
    rowsOf(readFileSync(blessedPath, "utf8")),
    names,
    treeOnly,
  );
  for (const row of moved) console.log(`${row.name}\t${row.where.join(", ")}`);
  if (problems.length > 0) {
    for (const problem of problems) console.error(`✗ ${problem}`);
    console.error(`recorded-bless-check: ${problems.length} difference(s) a bless may not make`);
    return 1;
  }
  console.log(`recorded-bless-check: ${moved.length} row(s) moved, each only where a bless may`);
  return 0;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = main(process.argv.slice(2));
}
