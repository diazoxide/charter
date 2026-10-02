// Folding `changes/*.md` into CHANGELOG.md's `## [Unreleased]`. Run with `node --test tools/`.
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { fold, foldInto, pending } from "./changelog-fold.mjs";

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
  const folded = fold(log, fragments);
  for (const { text } of fragments) {
    for (const line of text.split("\n").filter((l) => l.startsWith("- "))) {
      assert.ok(folded.includes(line), `folded changelog lacks: ${line}`);
    }
  }
});
