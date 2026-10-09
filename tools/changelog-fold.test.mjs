// Folding `changes/*.md` into CHANGELOG.md's `## [Unreleased]`. Run with `node --test tools/`.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { STANDING, fold, foldInto, mode, pending, standing } from "./changelog-fold.mjs";

const LOG = `# Changelog

Intro.

## [Unreleased]

### Added

- **An older addition.** Already here.

### Fixed

- **An older fix.**

## [0.1.0] - 2026-09-01

### Added

- **The first release.**

[Unreleased]: https://example.com/compare/v0.1.0...HEAD
`;

test("a fragment's entry goes first under its heading in Unreleased", () => {
  const got = fold(LOG, [{ name: "new-thing.md", text: "### Added\n\n- **A new thing.**\n" }]);
  assert.equal(
    got,
    LOG.replace(
      "### Added\n\n- **An older addition.**",
      "### Added\n\n- **A new thing.**\n\n- **An older addition.**",
    ),
  );
});

test("fragments are taken in name order, the first one on top", () => {
  const got = fold(LOG, [
    { name: "b.md", text: "### Fixed\n\n- **Second.**\n" },
    { name: "a.md", text: "### Fixed\n\n- **First.**\n" },
  ]);
  assert.match(got, /### Fixed\n\n- \*\*First\.\*\*\n\n- \*\*Second\.\*\*\n\n- \*\*An older fix\.\*\*\n\n## \[0\.1\.0\]/);
});

test("a heading Unreleased lacks is made in Keep a Changelog's order", () => {
  const got = fold(LOG, [
    { name: "x.md", text: "### Changed\n\n- **Changed.**\n\n### Security\n\n- **Secured.**\n" },
  ]);
  assert.equal(
    got,
    LOG.replace("### Fixed", "### Changed\n\n- **Changed.**\n\n### Fixed").replace(
      "- **An older fix.**\n",
      "- **An older fix.**\n\n### Security\n\n- **Secured.**\n",
    ),
  );
});

test("an empty Unreleased gets the heading and the entry before the next version", () => {
  const empty = "# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-09-01\n\n- x\n";
  assert.equal(
    fold(empty, [{ name: "a.md", text: "### Added\n- **New.**\n" }]),
    "# Changelog\n\n## [Unreleased]\n\n### Added\n\n- **New.**\n\n## [0.1.0] - 2026-09-01\n\n- x\n",
  );
});

test("an entry of several lines and paragraphs is kept as written", () => {
  const entry = "- **A long one.** It wraps\n  onto a second line.\n\n  And a paragraph.";
  const got = fold(LOG, [{ name: "a.md", text: `### Added\n\n${entry}\n\n` }]);
  assert.ok(got.includes(`### Added\n\n${entry}\n\n- **An older addition.**`));
});

test("no fragments leaves the changelog as it is", () => {
  assert.equal(fold(LOG, []), LOG);
});

test("a fragment a reader could not place is refused, naming the file", () => {
  const refused = (text) => () => fold(LOG, [{ name: "bad.md", text }]);
  assert.throws(refused("- **No heading.**\n"), /bad\.md: the entry comes before/);
  assert.throws(refused("### Improved\n\n- x\n"), /bad\.md: "### Improved" is not a changelog heading/);
  assert.throws(refused("## Added\n\n- x\n"), /bad\.md: "## Added" is not/);
  assert.throws(refused("### Added\n\n"), /bad\.md: "### Added" has no entry/);
  assert.throws(refused(""), /bad\.md: no "### <heading>" line/);
});

test("a changelog with no Unreleased section is refused", () => {
  assert.throws(
    () => fold("# Changelog\n\n## [0.1.0]\n", [{ name: "a.md", text: "### Added\n- x\n" }]),
    /no `## \[Unreleased\]`/,
  );
});

test("folding a project's changes/ writes CHANGELOG.md and deletes the fragments, not the README", () => {
  const root = mkdtempSync(join(tmpdir(), "changelog-fold-"));
  mkdirSync(join(root, "changes"));
  writeFileSync(join(root, "CHANGELOG.md"), LOG);
  writeFileSync(join(root, "changes", "README.md"), "# How to write a fragment\n");
  writeFileSync(join(root, "changes", "new-thing.md"), "### Added\n\n- **A new thing.**\n");

  assert.deepEqual(foldInto(root), ["new-thing.md"]);
  assert.ok(readFileSync(join(root, "CHANGELOG.md"), "utf8").includes("- **A new thing.**\n\n- **An older addition.**"));
  assert.deepEqual(readdirSync(join(root, "changes")), ["README.md"]);
});

test("a refused fragment leaves CHANGELOG.md and every fragment in place", () => {
  const root = mkdtempSync(join(tmpdir(), "changelog-fold-"));
  mkdirSync(join(root, "changes"));
  writeFileSync(join(root, "CHANGELOG.md"), LOG);
  writeFileSync(join(root, "changes", "a-good.md"), "### Added\n\n- **Good.**\n");
  writeFileSync(join(root, "changes", "b-bad.md"), "- **No heading.**\n");

  assert.throws(() => foldInto(root), /b-bad\.md/);
  assert.equal(readFileSync(join(root, "CHANGELOG.md"), "utf8"), LOG);
  assert.deepEqual(readdirSync(join(root, "changes")).sort(), ["a-good.md", "b-bad.md"]);
});

test("the repository's own CHANGELOG.md and changes/ fold cleanly", () => {
  // Every fragment a pull request adds is folded here, in CI, before it reaches release prep.
  const root = fileURLToPath(new URL("..", import.meta.url));
  const fragments = pending(root).map((name) => ({
    name,
    text: readFileSync(join(root, "changes", name), "utf8"),
  }));
  const log = readFileSync(join(root, "CHANGELOG.md"), "utf8");
  const notice = standing(root);
  const folded = fold(log, fragments, notice);
  for (const { text } of notice ? [...fragments, notice] : fragments) {
    for (const line of text.split("\n").filter((l) => l.startsWith("- "))) {
      assert.ok(folded.includes(line), `folded changelog lacks: ${line}`);
    }
  }
});

test("an unknown flag such as --help folds nothing and is a usage error", () => {
  assert.deepEqual(mode(["--help"]), { run: "help" });
  assert.deepEqual(mode(["-h"]), { run: "help" });
  assert.deepEqual(mode(["--dry-run"]), { run: "usage", bad: "--dry-run" });
  assert.deepEqual(mode(["--check", "extra"]), { run: "usage", bad: "extra" });
  assert.deepEqual(mode([]), { run: "fold" });
  assert.deepEqual(mode(["--check"]), { run: "check" });
});

test("a file in changes/ that is not README.md or a .md fragment is refused, naming it", () => {
  const root = mkdtempSync(join(tmpdir(), "fold-"));
  mkdirSync(join(root, "changes"));
  writeFileSync(join(root, "CHANGELOG.md"), LOG);
  writeFileSync(join(root, "changes", "README.md"), "how\n");
  writeFileSync(join(root, "changes", "lost.markdown"), "### Fixed\n\n- x\n");
  assert.throws(() => pending(root), /lost\.markdown/);
  assert.equal(readFileSync(join(root, "CHANGELOG.md"), "utf8"), LOG);
});

test("run through a symlinked path, the script still acts and does not silently pass", () => {
  const link = join(mkdtempSync(join(tmpdir(), "fold-link-")), "tools");
  symlinkSync(fileURLToPath(new URL(".", import.meta.url)), link);
  const ran = spawnSync(process.execPath, [join(link, "changelog-fold.mjs"), "--bogus"]);
  assert.equal(ran.status, 2, String(ran.stderr));
});

const NOTICE = "### Deprecated\n\n- **The old names go at 1.0.** Every version says so.\n";

test("a standing notice goes under Unreleased on every fold, and only once", () => {
  const notice = { name: STANDING, text: NOTICE };
  const once = fold(LOG, [{ name: "a.md", text: "### Added\n\n- **New.**\n" }], notice);
  assert.ok(
    once.includes("### Deprecated\n\n- **The old names go at 1.0.** Every version says so.\n\n### Fixed"),
    once,
  );
  assert.equal(fold(once, [], notice), once);
  // A release renames Unreleased; the next fold puts the notice in the new Unreleased too.
  const released = once.replace("## [Unreleased]", "## [Unreleased]\n\n## [0.2.0] - 2026-10-01");
  const next = fold(released, [], notice);
  assert.equal(next.split("- **The old names go at 1.0.**").length - 1, 2);
  assert.ok(next.indexOf("- **The old names go at 1.0.**") < next.indexOf("## [0.2.0]"));
});

test("folding keeps the standing notice in changes/ and --check does not wait on it", () => {
  const root = mkdtempSync(join(tmpdir(), "changelog-fold-"));
  mkdirSync(join(root, "changes"));
  writeFileSync(join(root, "CHANGELOG.md"), LOG);
  writeFileSync(join(root, "changes", "README.md"), "how\n");
  writeFileSync(join(root, "changes", STANDING), NOTICE);
  writeFileSync(join(root, "changes", "new-thing.md"), "### Added\n\n- **A new thing.**\n");

  assert.deepEqual(pending(root), ["new-thing.md"]);
  assert.deepEqual(foldInto(root), ["new-thing.md"]);
  assert.ok(readFileSync(join(root, "CHANGELOG.md"), "utf8").includes("- **The old names go at 1.0.**"));
  assert.deepEqual(readdirSync(join(root, "changes")).sort(), [STANDING, "README.md"].sort());
  assert.deepEqual(pending(root), []);
});

test("a standing notice a reader could not place is refused, naming the file", () => {
  assert.throws(() => fold(LOG, [], { name: STANDING, text: "- **No heading.**\n" }), /STANDING\.md/);
});
