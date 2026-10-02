// The supported-platforms list is published twice, in `docs/platforms.md` (the site) and in the
// README, and each row names the CI label it is covered on (FR-25). These tests keep the two
// copies one list, and check that every CI label the table names is still used in `ci.yml`. They
// do not check which job uses it, or what that job proves; the table's prose says that.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { platformTable, ciLabelsOf, ciLabelsInWorkflow } from "../src/platforms.mjs";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("the README carries the platforms page's table, row for row", () => {
  const table = platformTable(read("docs/platforms.md"));
  assert.ok(table.length > 2, "docs/platforms.md has a platforms table");
  assert.deepEqual(platformTable(read("README.md")), table);
});

test("every CI label the platforms table names is still used in ci.yml", () => {
  const named = ciLabelsOf(platformTable(read("docs/platforms.md")));
  assert.ok(named.length > 0, "the table names at least one CI label");
  const workflow = ciLabelsInWorkflow(read(".github/workflows/ci.yml"));
  assert.deepEqual(
    named.filter((label) => !workflow.includes(label)),
    [],
  );
});

test("a table is read from the first header that starts with Platform", () => {
  const text = "# T\n\nprose\n\n| Platform | CI label |\n|---|---|\n| A | `x-1` |\n\nafter\n";
  assert.deepEqual(platformTable(text), ["| Platform | CI label |", "|---|---|", "| A | `x-1` |"]);
});

test("a CI label is a backticked name in the CI label column; a dash names none", () => {
  const table = [
    "| Platform | CI label | Proves |",
    "|---|---|---|",
    "| A | `macos-latest`, `xcode-27` | `not-a-label` |",
    "| B | — | x |",
  ];
  assert.deepEqual(ciLabelsOf(table), ["macos-latest", "xcode-27"]);
});

test("the workflow's CI labels are its runs-on labels, matrix entries and container images", () => {
  const yml = [
    "jobs:",
    "  a:",
    "    runs-on: windows-latest",
    "  b:",
    "    strategy:",
    "      matrix:",
    "        os: [macos-latest, ubuntu-24.04]",
    "    runs-on: ${{ matrix.os }}",
    "  c:",
    "    container:",
    "      image: fedora:latest",
    "    # os: [commented-out]",
  ].join("\n");
  assert.deepEqual(ciLabelsInWorkflow(yml).sort(), [
    "fedora:latest",
    "macos-latest",
    "ubuntu-24.04",
    "windows-latest",
  ]);
});
