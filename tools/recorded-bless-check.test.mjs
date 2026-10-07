// What a blessed recorded-behaviour file may differ in, and what it may not.
import assert from "node:assert/strict";
import { test } from "node:test";

import { check, rowsOf } from "./recorded-bless-check.mjs";

const row = (name, more = {}) => ({
  args: ["doctor"],
  name,
  stdin: "",
  start: {},
  expect: {
    exit: 0,
    stdout: { masks: [], rule: "exact", text: "one\n" },
    stderr: { masks: [], rule: "exact", text: "" },
    tree: { remove: [], set: {} },
    outside: { allowed: [], hit: [] },
    ...more,
  },
});

const said = (base, text) => ({
  ...base,
  expect: { ...base.expect, stdout: { ...base.expect.stdout, text } },
});

test("a named row that moved only in its stdout is allowed, and said", () => {
  const committed = [row("doctor-a"), row("init-b")];
  const blessed = [said(committed[0], "two\n"), committed[1]];
  const { moved, problems } = check(committed, blessed, ["doctor-"]);
  assert.deepEqual(problems, []);
  assert.deepEqual(moved, [{ name: "doctor-a", where: ["stdout.text"] }]);
});

test("a row that moved and was not named is refused", () => {
  const committed = [row("doctor-a"), row("init-b")];
  const blessed = [said(committed[0], "two\n"), said(committed[1], "two\n")];
  const { problems } = check(committed, blessed, ["doctor-"]);
  assert.deepEqual(problems, ["init-b: moved, and it is not a row that was named"]);
});

test("a named row whose verdict, command or masks moved is refused", () => {
  const committed = [row("doctor-a", { denies: "refused" })];
  for (const blessed of [
    [{ ...committed[0], expect: { ...committed[0].expect, denies: "allowed" } }],
    [{ ...committed[0], args: ["doctor", "--fix"] }],
    [{ ...committed[0], expect: { ...committed[0].expect, exit: 1 } }],
    [
      {
        ...committed[0],
        expect: { ...committed[0].expect, stdout: { masks: [["x", "y"]], rule: "exact", text: "one\n" } },
      },
    ],
  ]) {
    const { moved, problems } = check(committed, blessed, ["doctor-a"]);
    assert.deepEqual(moved, []);
    assert.match(problems[0], /^doctor-a: differs outside what a bless rewrites/);
  }
});

test("rows added, dropped, renamed or reordered are refused before anything else", () => {
  const committed = [row("doctor-a"), row("init-b")];
  for (const blessed of [
    [committed[0]],
    [committed[1], committed[0]],
    [committed[0], { ...committed[1], name: "init-c" }],
  ]) {
    const { problems } = check(committed, blessed, ["doctor-", "init-"]);
    assert.equal(problems.length, 1);
    assert.match(problems[0], /^the rows are not the same rows in the same order/);
  }
});

test("a name that was given and moved nothing is said, so a stale list is noticed", () => {
  const committed = [row("doctor-a")];
  const { problems } = check(committed, committed, ["doctor-a", "init-"]);
  assert.deepEqual(problems, [
    "doctor-a: named, and no such row moved",
    "init-: named, and no such row moved",
  ]);
});

test("with tree paths named, a row's tree may move only there", () => {
  const settings = ".claude/settings.json";
  const withTree = (base, set) => ({ ...base, expect: { ...base.expect, tree: { remove: [], set } } });
  const committed = [withTree(row("init-a"), { [settings]: { text: "old" }, "notes.md": { text: "n" } })];
  const allowed = [withTree(committed[0], { [settings]: { text: "new" }, "notes.md": { text: "n" } })];
  assert.deepEqual(check(committed, allowed, ["init-a"], [settings]).problems, []);
  const stray = [withTree(committed[0], { [settings]: { text: "new" }, "notes.md": { text: "x" } })];
  assert.deepEqual(check(committed, stray, ["init-a"], [settings]).problems, [
    "init-a: its tree moved at notes.md, which was not named",
  ]);
  // Without any path named, the whole tree is the row's to move.
  assert.deepEqual(check(committed, stray, ["init-a"]).problems, []);
});

test("a file is read a row to a line, and a line that is not JSON is named", () => {
  assert.deepEqual(rowsOf('{"name":"a"}\n{"name":"b"}\n'), [{ name: "a" }, { name: "b" }]);
  assert.throws(() => rowsOf('{"name":"a"}\nnope\n'), /line 2 is not one JSON object/);
});
