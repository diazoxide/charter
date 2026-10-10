// The scenario job's failed tests as GitHub annotations (`tools/e2e-annotations.mjs`). Run with
// `node --test tools/`.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { LIMIT, MESSAGE_CHARS, annotations, lineOf, readRecords } from "./e2e-annotations.mjs";

const script = join(dirname(fileURLToPath(import.meta.url)), "e2e-annotations.mjs");
const root = "/work/purlis";
const spec = `${root}/app/e2e/specs/tabs.e2e.ts`;
const noSource = () => "";

function failure(overrides = {}) {
  return {
    file: spec,
    parent: "tabs",
    title: "a closed tab is gone",
    message: "Expected 2 tabs, received 3",
    stack: `Error: Expected 2 tabs, received 3\n    at Context.<anonymous> (${spec}:42:11)\n    at node:internal/process/task_queues:95:5`,
    ...overrides,
  };
}

test("a failed test is an error on its spec's line, relative to the repository, with its message", () => {
  assert.deepEqual(annotations([failure()], { root, read: noSource }), [
    "::error file=app/e2e/specs/tabs.e2e.ts,line=42,title=tabs › a closed tab is gone::Expected 2 tabs, received 3",
  ]);
});

test("the line comes from a file:// frame too", () => {
  const record = failure({ stack: `Error: x\n    at file://${spec}:7:3` });
  assert.equal(lineOf(record, root, noSource), 7);
});

test("with no frame in the spec, the line that names the test is used, else 1", () => {
  const timeout = failure({
    stack: "Error: Timeout of 180000ms exceeded\n    at listOnTimeout (node:internal/timers:1:1)",
  });
  const source = 'describe("tabs", () => {\n  it("a closed tab is gone", async () => {\n';
  assert.equal(
    lineOf(timeout, root, () => source),
    2,
  );
  assert.equal(lineOf(timeout, root, noSource), 1);
});

test("colour codes go, a long message is cut, and newlines, % and the property separators are escaped", () => {
  const long = "x".repeat(MESSAGE_CHARS * 2);
  const [cut] = annotations([failure({ message: `\u001b[31m${long}\u001b[39m` })], {
    root,
    read: noSource,
  });
  const message = cut.slice(cut.indexOf("::", 2) + 2);
  assert.equal(message.length, MESSAGE_CHARS);
  assert.ok(message.endsWith("…"));

  const [escaped] = annotations(
    [failure({ title: "50%: a, b", message: "line one\nline two 100%" })],
    {
      root,
      read: noSource,
    },
  );
  assert.equal(
    escaped,
    "::error file=app/e2e/specs/tabs.e2e.ts,line=42,title=tabs › 50%25%3A a%2C b::line one%0Aline two 100%25",
  );
});

test("a mass failure is ten errors, ten warnings and a notice counting the rest", () => {
  const records = Array.from({ length: 35 }, (_, i) => failure({ title: `test ${i}` }));
  const lines = annotations(records, { root, read: noSource });
  assert.equal(lines.length, LIMIT + 1);
  assert.equal(lines.filter((l) => l.startsWith("::error ")).length, 10);
  assert.equal(lines.filter((l) => l.startsWith("::warning ")).length, 10);
  assert.match(lines[10], /title=also failed › tabs › test 10::/);
  assert.match(
    lines[LIMIT],
    /^::notice title=\+15 more failed scenario tests::15 more failed; 35 in all\./,
  );
});

test("a job that failed with no test recorded still says so", () => {
  const lines = annotations([], { root, read: noSource });
  assert.equal(lines.length, 1);
  assert.match(lines[0], /^::error title=scenario tests failed with no test recorded::/);
});

test("a record without a file or a message still annotates", () => {
  assert.deepEqual(annotations([{ title: '"before all" hook' }], { root, read: noSource }), [
    '::error title="before all" hook::failed with no message',
  ]);
});

test("a half-written line is skipped and the rest are read", () => {
  const text = `${JSON.stringify(failure())}\n{"file": "cut sh\n\n${JSON.stringify(failure({ title: "b" }))}\n`;
  assert.deepEqual(
    readRecords(text).map((r) => r.title),
    ["a closed tab is gone", "b"],
  );
});

test("run as a script, it prints the annotations for a failures file, or the no-test one for none", () => {
  const dir = mkdtempSync(join(tmpdir(), "e2e-annotations-"));
  const file = join(dir, "failures.jsonl");
  writeFileSync(
    file,
    `${JSON.stringify(failure({ file: "app/e2e/specs/tabs.e2e.ts", stack: "" }))}\n`,
  );
  const run = spawnSync(process.execPath, [script, file], { encoding: "utf8" });
  assert.equal(run.status, 0, run.stderr);
  assert.match(
    run.stdout,
    /^::error file=app\/e2e\/specs\/tabs\.e2e\.ts,line=\d+,title=tabs › a closed tab is gone::/,
  );

  const none = spawnSync(process.execPath, [script, join(dir, "missing.jsonl")], {
    encoding: "utf8",
  });
  assert.equal(none.status, 0, none.stderr);
  assert.match(none.stdout, /no test recorded/);
});
