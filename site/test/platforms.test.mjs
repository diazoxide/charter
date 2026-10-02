// The supported-platforms list is published twice, in `docs/platforms.md` (the site) and in the
// README, and each row names the CI runner that covers it (FR-25). These tests keep the two copies
// one list, and keep every runner the list names in `ci.yml`, so a row cannot claim coverage that
// was taken out of CI.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { platformTable, runnersOf, runnersInWorkflow } from "../src/platforms.mjs";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("the README carries the platforms page's table, row for row", () => {
  const table = platformTable(read("docs/platforms.md"));
  assert.ok(table.length > 2, "docs/platforms.md has a platforms table");
  assert.deepEqual(platformTable(read("README.md")), table);
});

test("every CI runner the platforms table names is one ci.yml runs on", () => {
  const named = runnersOf(platformTable(read("docs/platforms.md")));
  assert.ok(named.length > 0, "the table names at least one runner");
  const workflow = runnersInWorkflow(read(".github/workflows/ci.yml"));
  assert.deepEqual(
    named.filter((runner) => !workflow.includes(runner)),
    [],
  );
});

test("a table is read from the first header that starts with Platform", () => {
  const text = "# T\n\nprose\n\n| Platform | CI runner |\n|---|---|\n| A | `x-1` |\n\nafter\n";
  assert.deepEqual(platformTable(text), ["| Platform | CI runner |", "|---|---|", "| A | `x-1` |"]);
});

test("a runner is a backticked label in the CI runner column; a dash names none", () => {
  const table = [
    "| Platform | CI runner | Proves |",
    "|---|---|---|",
    "| A | `macos-latest`, `xcode-27` | `not-a-runner` |",
    "| B | — | x |",
  ];
  assert.deepEqual(runnersOf(table), ["macos-latest", "xcode-27"]);
});

test("the workflow's runners are its runs-on labels, matrix entries and container images", () => {
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
  assert.deepEqual(runnersInWorkflow(yml).sort(), [
    "fedora:latest",
    "macos-latest",
    "ubuntu-24.04",
    "windows-latest",
  ]);
});
