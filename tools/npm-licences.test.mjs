// The npm licence check (FM-3, #1106). Run with `node --test tools/`.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { allowedIn, check, satisfies, waysOutIn } from "./npm-licences.mjs";

const ALLOWED = [
  "MIT",
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "ISC",
  "0BSD",
];

test("reads the allow-list out of deny.toml's [licenses] table", () => {
  const toml = `[advisories]\nallow = ["not-this"]\n\n[licenses]\nversion = 2\nallow = [\n  "MIT",\n  # a reason\n  "Apache-2.0 WITH LLVM-exception",\n]\n\n[bans]\n`;
  assert.deepEqual(allowedIn(toml), ["MIT", "Apache-2.0 WITH LLVM-exception"]);
});

test("an SPDX expression is allowed when it can be met with allowed licences", () => {
  assert.equal(satisfies("MIT", ALLOWED), true);
  assert.equal(satisfies("(MIT OR GPL-3.0-or-later)", ALLOWED), true);
  assert.equal(satisfies("GPL-3.0-or-later OR Apache-2.0", ALLOWED), true);
  assert.equal(satisfies("(MIT AND ISC)", ALLOWED), true);
  assert.equal(satisfies("Apache-2.0 WITH LLVM-exception", ALLOWED), true);
  assert.equal(satisfies("MIT AND GPL-3.0-only", ALLOWED), false);
  assert.equal(satisfies("CC-BY-4.0", ALLOWED), false);
  assert.equal(
    satisfies("Apache-2.0 WITH Classpath-exception-2.0", ALLOWED),
    false,
  );
  assert.equal(satisfies("MIT OR", ALLOWED), false);
  assert.equal(satisfies("", ALLOWED), false);
  assert.equal(satisfies(undefined, ALLOWED), false);
});

/** A lockfile holding `packages`, keyed as npm keys them. */
const lock = (packages) => ({
  lockfileVersion: 3,
  packages: { "": { name: "app" }, ...packages },
});

test("fails a shipped package outside the allow-list, or one that names no licence", () => {
  const said = check({
    lock: lock({
      "node_modules/fine": { license: "MIT" },
      "node_modules/copyleft": { license: "GPL-3.0-only" },
      "node_modules/silent": {},
    }),
    allowed: ALLOWED,
    vendored: [],
  });
  assert.deepEqual(said.failures, [
    "node_modules/copyleft is GPL-3.0-only, which deny.toml does not allow",
    "node_modules/silent names no licence",
  ]);
});

test("reports a development-only package outside the list without failing on it", () => {
  const said = check({
    lock: lock({
      "node_modules/caniuse-lite": { license: "CC-BY-4.0", dev: true },
    }),
    allowed: ALLOWED,
    vendored: [],
  });
  assert.deepEqual(said.failures, []);
  assert.deepEqual(said.notes, [
    "node_modules/caniuse-lite is CC-BY-4.0 (development only)",
  ]);
});

test("fails a vendored asset outside the list, or one shipped without its licence text", () => {
  const root = mkdtempSync(join(tmpdir(), "npm-licences-"));
  mkdirSync(join(root, "kept"));
  writeFileSync(join(root, "kept", "LICENSE"), "The MIT License\n");
  mkdirSync(join(root, "bare"));
  const said = check({
    lock: lock({}),
    allowed: ALLOWED,
    vendored: [
      { name: "kept", licence: "MIT", files: "kept" },
      { name: "bare", licence: "MIT", files: "bare" },
      { name: "copyleft", licence: "GPL-3.0-only", files: "kept" },
    ],
    root,
  });
  assert.deepEqual(said.failures, [
    "the vendored bare has no LICENSE beside it in bare",
    "the vendored copyleft is GPL-3.0-only, which deny.toml does not allow",
  ]);
});

/** The exceptions file as `waysOutIn` reads it. */
const waysOut = (exceptions = [], clarify = []) =>
  waysOutIn(JSON.stringify({ exceptions, clarify }));

test("an exception passes only the package it names", () => {
  const said = check({
    lock: lock({
      "node_modules/fonts": { version: "1.0.0", license: "CC-BY-4.0" },
      "node_modules/other": { version: "2.0.0", license: "CC-BY-4.0" },
      "node_modules/@scope/nested/node_modules/fonts": {
        version: "0.9.0",
        license: "CC-BY-4.0",
      },
    }),
    allowed: ALLOWED,
    vendored: [],
    waysOut: waysOut([
      { name: "fonts", allow: ["CC-BY-4.0"], reason: "glyph data" },
    ]),
  });
  assert.deepEqual(said.failures, [
    "node_modules/other is CC-BY-4.0, which deny.toml does not allow",
  ]);
});

test("an exception allows only its own licences, and a package's other terms still count", () => {
  const said = check({
    lock: lock({
      "node_modules/fonts": {
        version: "1.0.0",
        license: "CC-BY-4.0 AND GPL-3.0-only",
      },
    }),
    allowed: ALLOWED,
    vendored: [],
    waysOut: waysOut([
      { name: "fonts", allow: ["CC-BY-4.0"], reason: "glyph data" },
    ]),
  });
  assert.deepEqual(said.failures, [
    "node_modules/fonts is CC-BY-4.0 AND GPL-3.0-only, which deny.toml does not allow",
    "the exception for fonts is stale: it lets no package in the lock pass",
  ]);
});

test("a clarification overrides only the version it names", () => {
  const said = check({
    lock: lock({
      "node_modules/quiet": { version: "1.2.3" },
      "node_modules/a/node_modules/quiet": { version: "1.2.4" },
    }),
    allowed: ALLOWED,
    vendored: [],
    waysOut: waysOut(
      [],
      [
        {
          name: "quiet",
          version: "1.2.3",
          licence: "MIT",
          reason: "its LICENSE file is MIT",
        },
      ],
    ),
  });
  assert.deepEqual(said.failures, [
    "node_modules/a/node_modules/quiet names no licence",
  ]);
});

test("a clarification is held to the list like the lock's own word", () => {
  const said = check({
    lock: lock({ "node_modules/quiet": { version: "1.2.3", license: "MIT" } }),
    allowed: ALLOWED,
    vendored: [],
    waysOut: waysOut(
      [],
      [
        {
          name: "quiet",
          version: "1.2.3",
          licence: "GPL-3.0-only",
          reason: "relicensed",
        },
      ],
    ),
  });
  assert.deepEqual(said.failures, [
    "node_modules/quiet is GPL-3.0-only (clarified), which deny.toml does not allow",
  ]);
});

test("an entry nothing in the lock uses is reported as stale, and fails", () => {
  const said = check({
    lock: lock({
      "node_modules/fine": { version: "1.0.0", license: "MIT" },
      "node_modules/quiet": { version: "1.2.4" },
    }),
    allowed: ALLOWED,
    vendored: [],
    waysOut: waysOut(
      [
        { name: "gone", allow: ["CC-BY-4.0"], reason: "was a dependency" },
        {
          name: "fine",
          allow: ["CC-BY-4.0"],
          reason: "its licence changed since",
        },
      ],
      [
        {
          name: "quiet",
          version: "1.2.3",
          licence: "MIT",
          reason: "an older release",
        },
        {
          name: "fine",
          version: "1.0.0",
          licence: "MIT",
          reason: "the lock says so now",
        },
      ],
    ),
  });
  assert.deepEqual(said.failures, [
    "node_modules/quiet names no licence",
    "the exception for gone is stale: it lets no package in the lock pass",
    "the exception for fine is stale: it lets no package in the lock pass",
    "the clarification of quiet@1.2.3 is stale: no package in the lock is that version and says otherwise",
    "the clarification of fine@1.0.0 is stale: no package in the lock is that version and says otherwise",
  ]);
});

test("an exception for a development-only package is used by it, and silences its note", () => {
  const said = check({
    lock: lock({
      "node_modules/caniuse-lite": {
        version: "1.0.0",
        license: "CC-BY-4.0",
        dev: true,
      },
    }),
    allowed: ALLOWED,
    vendored: [],
    waysOut: waysOut([
      { name: "caniuse-lite", allow: ["CC-BY-4.0"], reason: "data" },
    ]),
  });
  assert.deepEqual(said, { failures: [], notes: [] });
});

test("the exceptions file is read strictly: one package each, an exact version, a reason", () => {
  const read = waysOutIn(
    JSON.stringify({
      exceptions: [
        { name: "a", allow: ["X"] },
        { name: "", allow: ["X"], reason: "r" },
        { name: "b", allow: [], reason: "r" },
        { name: "c", allow: ["X"], reason: "r" },
        { name: "c", allow: ["Y"], reason: "r" },
      ],
      clarify: [
        { name: "d", version: "^1.2.3", licence: "MIT", reason: "r" },
        { name: "e", version: "1.x", licence: "MIT", reason: "r" },
        { name: "f", version: "1.0.0", reason: "r" },
        { name: "g", version: "1.0.0-beta.1", licence: "MIT", reason: "r" },
      ],
      extra: [],
    }),
  );
  assert.deepEqual(read.problems, [
    "npm-licences.json has extra, which it does not know",
    "npm-licences.json exceptions[0] needs a name, a reason, and the licences it allows",
    "npm-licences.json exceptions[1] needs a name, a reason, and the licences it allows",
    "npm-licences.json exceptions[2] needs a name, a reason, and the licences it allows",
    "npm-licences.json exceptions[4] names c again",
    "npm-licences.json clarify[0] needs a name, an exact version, a licence and a reason",
    "npm-licences.json clarify[1] needs a name, an exact version, a licence and a reason",
    "npm-licences.json clarify[2] needs a name, an exact version, a licence and a reason",
  ]);
  assert.deepEqual(
    read.exceptions.map((one) => one.name),
    ["c"],
  );
  assert.deepEqual(
    read.clarify.map((one) => `${one.name}@${one.version}`),
    ["g@1.0.0-beta.1"],
  );
  assert.deepEqual(
    check({
      lock: lock({}),
      allowed: ALLOWED,
      vendored: [],
      waysOut: read,
    }).failures.slice(0, 1),
    ["npm-licences.json has extra, which it does not know"],
  );
});

test("an exceptions file that is not JSON fails the check, and none at all is no exception", () => {
  assert.deepEqual(waysOutIn("{ not json").problems, [
    "npm-licences.json is not JSON",
  ]);
  assert.deepEqual(waysOutIn(undefined), {
    exceptions: [],
    clarify: [],
    problems: [],
  });
});

test("this repository's app passes, as CI runs it", () => {
  const script = fileURLToPath(new URL("./npm-licences.mjs", import.meta.url));
  const ran = spawnSync(process.execPath, [script], { encoding: "utf8" });
  assert.equal(ran.status, 0, ran.stderr + ran.stdout);
});
