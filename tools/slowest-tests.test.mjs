// Which Rust tests and test binaries took longest, read from nextest's JUnit report. Run with
// `node --test tools/`.
import assert from "node:assert/strict";
import { test } from "node:test";

import { annotations, readCases, slowest } from "./slowest-tests.mjs";

/** A report as nextest writes it: one testsuite per test binary, a testcase per test. */
const report = `<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="4" failures="0" errors="0" time="12.5">
    <testsuite name="purlis-cli::cli" tests="2" disabled="0" errors="0" failures="0">
        <testcase name="a_quick_one" classname="purlis-cli::cli" timestamp="2026-10-10T10:00:00Z" time="0.010">
        </testcase>
        <testcase name="waits &amp; waits" classname="purlis-cli::cli" timestamp="2026-10-10T10:00:00Z" time="9.500">
            <system-out>long output</system-out>
        </testcase>
    </testsuite>
    <testsuite name="fake-harness::unread_answers" tests="2" disabled="0" errors="0" failures="0">
        <testcase time="3.000" name="floods" classname="fake-harness::unread_answers"/>
        <testcase name="floods_again" classname="fake-harness::unread_answers" time="2.250"></testcase>
    </testsuite>
</testsuites>`;

test("every testcase is read, whatever order its attributes come in", () => {
  assert.deepEqual(readCases(report), [
    { binary: "purlis-cli::cli", name: "a_quick_one", seconds: 0.01 },
    { binary: "purlis-cli::cli", name: "waits & waits", seconds: 9.5 },
    { binary: "fake-harness::unread_answers", name: "floods", seconds: 3 },
    { binary: "fake-harness::unread_answers", name: "floods_again", seconds: 2.25 },
  ]);
});

test("a report with no tests reads as none", () => {
  assert.deepEqual(readCases(""), []);
});

test("tests come slowest first, and binaries by the time their tests took in all", () => {
  const { tests, binaries } = slowest(readCases(report), { limit: 2 });
  assert.deepEqual(
    tests.map((one) => one.name),
    ["waits & waits", "floods"],
  );
  assert.deepEqual(binaries, [
    { binary: "purlis-cli::cli", seconds: 9.51, tests: 2 },
    { binary: "fake-harness::unread_answers", seconds: 5.25, tests: 2 },
  ]);
});

test("two notices: the slowest tests and the slowest binaries, one per line", () => {
  const lines = annotations(readCases(report));
  assert.equal(lines.length, 2);
  assert.match(lines[0], /^::notice title=Slowest Rust tests::/);
  assert.match(lines[0], /9\.5 s {2}purlis-cli::cli {2}waits & waits%0A/);
  assert.match(lines[1], /^::notice title=Slowest Rust test binaries \(their tests' time summed\)::/);
  assert.match(lines[1], /9\.5 s {2}purlis-cli::cli \(2 tests\)/);
});

test("a name that holds a workflow command's own characters is escaped", () => {
  const [tests] = annotations([{ binary: "x::y", name: "50% done\nnext", seconds: 1 }]);
  assert.match(tests, /x::y {2}50%25 done%0Anext$/);
});

test("a run that wrote no report says so, and does not fail", () => {
  assert.deepEqual(annotations([]), ["::notice title=Slowest Rust tests::No JUnit report with tests was found."]);
});
