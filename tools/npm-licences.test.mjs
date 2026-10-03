// The npm licence check (FM-3, #1106). Run with `node --test tools/`.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { allowedIn, check, satisfies } from "./npm-licences.mjs";

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

test("this repository's app passes, as CI runs it", () => {
  const script = fileURLToPath(new URL("./npm-licences.mjs", import.meta.url));
  const ran = spawnSync(process.execPath, [script], { encoding: "utf8" });
  assert.equal(ran.status, 0, ran.stderr + ran.stdout);
});
