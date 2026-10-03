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

/** What is outside the list (`failures`), and what is outside it but never shipped (`notes`). */
export function check({ lock, allowed, vendored, root = "." }) {
  const failures = [];
  const notes = [];
  for (const [path, entry] of Object.entries(lock.packages ?? {})) {
    if (path === "" || entry.link === true) continue;
    const licence = licenceOf(entry);
    if (satisfies(licence, allowed)) continue;
    if (entry.dev === true)
      notes.push(`${path} is ${licence ?? "unlicensed"} (development only)`);
    else if (licence === undefined) failures.push(`${path} names no licence`);
    else failures.push(`${path} is ${licence}, which deny.toml does not allow`);
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
  });
  for (const note of said.notes) console.log(`note: ${note}`);
  for (const failure of said.failures) console.error(`error: ${failure}`);
  if (said.failures.length > 0) process.exit(1);
  console.log(
    "every shipped npm package and vendored asset is under a licence deny.toml allows",
  );
}
