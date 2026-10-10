// The npm licence check (FM-3, #1106): every npm package the app ships, and every asset vendored
// into it, is under a licence `deny.toml` allows — the one allow-list, for crates and npm alike.
//
//   node tools/npm-licences.mjs   exit 1 naming each package or asset outside the list
//
// **What it holds to the list.** Every package in `app/package-lock.json` that is not
// development-only (npm's `dev: true`), which is what reaches the app's bundle, and every entry
// of `app/icons/vendored.json`, which must also carry its licence text beside it. A
// development-only package outside the list is reported and never fails: the build tools
// (caniuse-lite's CC-BY-4.0, argparse's Python-2.0) are not shipped, and holding them to a
// shipping list would mean widening the list for things nobody receives.
//
// An SPDX expression is met when the licences it can be taken under are all allowed: `A OR B`
// needs one of them, `A AND B` both, and `A WITH E` is allowed only as that exact pair.
//
// **The two ways out, short of widening the list** (#1145), the npm half of `deny.toml`'s
// `[[licenses.exceptions]]` and `[[licenses.clarify]]`, kept in `npm-licences.json` beside it
// (cargo-deny reads those two tables as crate names, so npm's cannot share them):
//
//   { "exceptions": [{ "name": "<package>", "allow": ["<licence>", ...], "reason": "..." }],
//     "clarify": [{ "name": "<package>", "version": "<exact>", "licence": "<SPDX>",
//                   "reason": "..." }] }
//
// An exception allows its licences for the one package it names, at any version, and for no
// other. A clarification says what one exact version of a package is really under, where its
// lockfile entry names no licence or the wrong one; what it says is then held to the list like
// the lock's own word. Each entry gives its reason, as each line of `deny.toml` does in a
// comment. An entry no package in the lock needs, or a clarification the lock already agrees
// with, is stale and fails the check: left, it would cover whatever later takes that name.
import { existsSync, readFileSync, realpathSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

/** The strings of `deny.toml`'s `[licenses] allow = [...]`, comments left out. */
export function allowedIn(toml) {
  const table =
    /^\[licenses\]\s*$([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(toml)?.[1] ?? "";
  const list = /^allow\s*=\s*\[([\s\S]*?)\]/m.exec(table)?.[1] ?? "";
  return [...list.replace(/#[^\n]*/g, "").matchAll(/"([^"]+)"/g)].map(
    (hit) => hit[1],
  );
}

/** Whether the SPDX `expression` can be met with licences in `allowed`. */
export function satisfies(expression, allowed) {
  if (typeof expression !== "string") return false;
  const words = expression.match(/\(|\)|[^\s()]+/g) ?? [];
  let at = 0;
  // or := and ("OR" and)* ; and := one ("AND" one)* ; one := "(" or ")" | id ["WITH" id]
  const one = () => {
    if (words[at] === "(") {
      at += 1;
      const met = or();
      if (words[at] !== ")") throw new Error("unbalanced");
      at += 1;
      return met;
    }
    const id = words[at];
    if (id === undefined || ["AND", "OR", "WITH", ")"].includes(id))
      throw new Error("no licence");
    at += 1;
    if (words[at] === "WITH") {
      const exception = words[at + 1];
      if (exception === undefined) throw new Error("no exception");
      at += 2;
      return allowed.includes(`${id} WITH ${exception}`);
    }
    return allowed.includes(id);
  };
  const and = () => {
    let met = one();
    while (words[at] === "AND") {
      at += 1;
      met = one() && met;
    }
    return met;
  };
  const or = () => {
    let met = and();
    while (words[at] === "OR") {
      at += 1;
      met = and() || met;
    }
    return met;
  };
  try {
    const met = or();
    return at === words.length && met;
  } catch {
    return false;
  }
}

/** A lockfile entry's licence: npm writes a string, and an old package an object. */
function licenceOf(entry) {
  if (typeof entry.license === "string") return entry.license;
  if (entry.license !== null && typeof entry.license === "object")
    return entry.license.type;
  return undefined;
}

/** The file the exceptions and clarifications are kept in, beside `deny.toml`. */
export const WAYS_OUT = "npm-licences.json";

const nonEmpty = (value) => typeof value === "string" && value.trim() !== "";

/** One exact version, as npm writes it in a lock: no range, wildcard or tag. */
const EXACT = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

/**
 * The exceptions and clarifications in `text` (the file's contents, or `undefined` where there
 * is none), with every entry that is not one of them as a `problems` line, which fails the check.
 */
export function waysOutIn(text) {
  const read = { exceptions: [], clarify: [], problems: [] };
  if (text === undefined) return read;
  let file;
  try {
    file = JSON.parse(text);
  } catch {
    read.problems.push(`${WAYS_OUT} is not JSON`);
    return read;
  }
  if (file === null || typeof file !== "object" || Array.isArray(file)) {
    read.problems.push(`${WAYS_OUT} is not an object`);
    return read;
  }
  for (const key of Object.keys(file)) {
    if (key !== "exceptions" && key !== "clarify")
      read.problems.push(`${WAYS_OUT} has ${key}, which it does not know`);
  }
  const list = (key) => {
    const value = file[key] ?? [];
    if (Array.isArray(value)) return value;
    read.problems.push(`${WAYS_OUT} ${key} is not a list`);
    return [];
  };
  const named = new Set();
  list("exceptions").forEach((one, at) => {
    const whole =
      nonEmpty(one?.name) &&
      nonEmpty(one.reason) &&
      Array.isArray(one.allow) &&
      one.allow.length > 0 &&
      one.allow.every(nonEmpty);
    if (!whole)
      read.problems.push(
        `${WAYS_OUT} exceptions[${at}] needs a name, a reason, and the licences it allows`,
      );
    else if (named.has(one.name))
      read.problems.push(
        `${WAYS_OUT} exceptions[${at}] names ${one.name} again`,
      );
    else {
      named.add(one.name);
      read.exceptions.push({ name: one.name, allow: [...one.allow] });
    }
  });
  const pinned = new Set();
  list("clarify").forEach((one, at) => {
    const whole =
      nonEmpty(one?.name) &&
      nonEmpty(one.reason) &&
      nonEmpty(one.licence) &&
      typeof one.version === "string" &&
      EXACT.test(one.version);
    const key = whole ? `${one.name}@${one.version}` : "";
    if (!whole)
      read.problems.push(
        `${WAYS_OUT} clarify[${at}] needs a name, an exact version, a licence and a reason`,
      );
    else if (pinned.has(key))
      read.problems.push(`${WAYS_OUT} clarify[${at}] names ${key} again`);
    else {
      pinned.add(key);
      read.clarify.push({
        name: one.name,
        version: one.version,
        licence: one.licence,
      });
    }
  });
  return read;
}

/** A lockfile entry's package name: its own `name` (an alias's), or the path's last part. */
function nameOf(path, entry) {
  if (typeof entry.name === "string") return entry.name;
  const at = path.lastIndexOf("node_modules/");
  return at === -1 ? path : path.slice(at + "node_modules/".length);
}

/** What is outside the list (`failures`), and what is outside it but never shipped (`notes`). */
export function check({
  lock,
  allowed,
  vendored,
  root = ".",
  waysOut = { exceptions: [], clarify: [], problems: [] },
}) {
  const failures = [...waysOut.problems];
  const notes = [];
  const excepted = new Set();
  const clarified = new Set();
  for (const [path, entry] of Object.entries(lock.packages ?? {})) {
    if (path === "" || entry.link === true) continue;
    const name = nameOf(path, entry);
    const clarification = waysOut.clarify.find(
      (one) => one.name === name && one.version === entry.version,
    );
    let licence = licenceOf(entry);
    // A clarification the lock already agrees with changes nothing, so nothing uses it.
    if (clarification !== undefined && clarification.licence !== licence) {
      clarified.add(clarification);
      licence = clarification.licence;
    }
    const shown =
      clarification !== undefined && clarified.has(clarification)
        ? `${licence} (clarified)`
        : licence;
    if (satisfies(licence, allowed)) continue;
    const exception = waysOut.exceptions.find((one) => one.name === name);
    if (
      exception !== undefined &&
      satisfies(licence, [...allowed, ...exception.allow])
    ) {
      excepted.add(exception);
      continue;
    }
    if (entry.dev === true)
      notes.push(`${path} is ${shown ?? "unlicensed"} (development only)`);
    else if (licence === undefined) failures.push(`${path} names no licence`);
    else failures.push(`${path} is ${shown}, which deny.toml does not allow`);
  }
  for (const asset of vendored) {
    if (!satisfies(asset.licence, allowed)) {
      failures.push(
        `the vendored ${asset.name} is ${asset.licence}, which deny.toml does not allow`,
      );
    } else if (!existsSync(join(root, asset.files, "LICENSE"))) {
      failures.push(
        `the vendored ${asset.name} has no LICENSE beside it in ${asset.files}`,
      );
    }
  }
  for (const one of waysOut.exceptions) {
    if (!excepted.has(one))
      failures.push(
        `the exception for ${one.name} is stale: it lets no package in the lock pass`,
      );
  }
  for (const one of waysOut.clarify) {
    if (!clarified.has(one))
      failures.push(
        `the clarification of ${one.name}@${one.version} is stale: no package in the lock is that version and says otherwise`,
      );
  }
  return { failures, notes };
}

/** Whether this file is the script node was asked to run, however the path to it was spelled. */
function runAsScript() {
  try {
    return (
      realpathSync(process.argv[1]) ===
      realpathSync(fileURLToPath(import.meta.url))
    );
  } catch {
    return false;
  }
}

if (runAsScript()) {
  const repo = fileURLToPath(new URL("..", import.meta.url));
  const read = (path) => readFileSync(join(repo, path), "utf8");
  const said = check({
    lock: JSON.parse(read("app/package-lock.json")),
    allowed: allowedIn(read("deny.toml")),
    vendored: JSON.parse(read("app/icons/vendored.json")),
    root: join(repo, "app/icons"),
    waysOut: waysOutIn(
      existsSync(join(repo, WAYS_OUT)) ? read(WAYS_OUT) : undefined,
    ),
  });
  for (const note of said.notes) console.log(`note: ${note}`);
  for (const failure of said.failures) console.error(`error: ${failure}`);
  if (said.failures.length > 0) process.exit(1);
  console.log(
    "every shipped npm package and vendored asset is under a licence deny.toml allows",
  );
}
