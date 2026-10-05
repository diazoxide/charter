// The nightly mutation report's decisions (#480): what is red, what is a hang, and what the
// nightly's one issue is told. `tools/mutants-report.py` is driven through its command line, the
// interface `.github/workflows/mutants.yml` uses. Run with `node --test tools/`.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const SCRIPT = join(dirname(fileURLToPath(import.meta.url)), "mutants-report.py");

function report(args, { input } = {}) {
  const run = spawnSync("python3", [SCRIPT, ...args], {
    encoding: "utf8",
    input,
    env: { ...process.env, GITHUB_STEP_SUMMARY: "" },
  });
  return { code: run.status, out: run.stdout, err: run.stderr };
}

// One mutant's outcome, as cargo-mutants 27 writes it into `outcomes.json`.
function outcome(summary, name, testSeconds) {
  const file = name.split(":")[0];
  return {
    scenario: { Mutant: { name, file } },
    summary,
    phase_results: [
      { phase: "Build", duration: 90 },
      { phase: "Test", duration: testSeconds },
    ],
  };
}

// A run's artifacts directory, holding one shard's verdict per entry of `shards`.
function run(shards, knownLines = []) {
  const root = mkdtempSync(join(tmpdir(), "charter-mutants-"));
  const artifacts = join(root, "artifacts");
  shards.forEach((outcomes, n) => {
    const out = join(root, `out-${n}`);
    mkdirSync(out);
    writeFileSync(join(out, "mutants.json"), JSON.stringify(outcomes.map(() => ({}))));
    writeFileSync(join(out, "outcomes.json"), JSON.stringify({ outcomes }));
    mkdirSync(join(artifacts, `mutants-${n}`), { recursive: true });
    const shard = report([
      "shard",
      "--output",
      out,
      "--shard",
      String(n),
      "--verdict",
      join(artifacts, `mutants-${n}`, `verdict-${n}.json`),
    ]);
    assert.equal(shard.code, 0, shard.out + shard.err);
  });
  const known = join(root, "known.txt");
  writeFileSync(known, knownLines.map((line) => line + "\n").join(""));
  const gather = (...extra) =>
    report([
      "gather",
      "--verdicts",
      artifacts,
      "--shards",
      String(shards.length),
      "--known",
      known,
      "--write-current",
      join(root, "current.txt"),
      ...extra,
    ]);
  return { gather, current: () => readFileSync(join(root, "current.txt"), "utf8") };
}

const MISSED = "crates/charter-core/src/a.rs:1:1: replace f -> bool with true";
const HUNG = "crates/charter-core/src/b.rs:2:3: replace += with *= in walk";

test("a mutant that outlives a timeout well above the whole suite is a hang, and not red", () => {
  const { gather, current } = run(
    [
      [
        outcome("CaughtMutant", "crates/charter-core/src/c.rs:1:1: delete ! in g", 20),
        outcome("MissedMutant", MISSED, 140),
        outcome("Timeout", HUNG, 240),
      ],
    ],
    ["crates/charter-core/src/a.rs: replace f -> bool with true"],
  );
  const result = gather("--timeout", "240");
  assert.equal(result.code, 0, result.out);
  assert.match(result.out, /## 1 hang/);
  assert.match(result.out, /`crates\/charter-core\/src\/b\.rs: replace \+= with \*= in walk`/);
  // The hang is caught, so it is not a survivor to write down either.
  assert.equal(current().trim(), "crates/charter-core/src/a.rs: replace f -> bool with true");
});

test("a timeout on a shard whose suite nears the limit is a suite cut short, and stays red", () => {
  const { gather, current } = run(
    [[outcome("MissedMutant", MISSED, 200), outcome("Timeout", HUNG, 240)]],
    ["crates/charter-core/src/a.rs: replace f -> bool with true"],
  );
  const result = gather("--timeout", "240");
  assert.equal(result.code, 1, result.out);
  assert.doesNotMatch(result.out, /## \d+ hang/);
  assert.match(result.out, /too close to the 240 s limit/);
  assert.match(result.out, /\| Timeout \| `crates\/charter-core\/src\/b\.rs` \|/);
  assert.match(current(), /b\.rs: replace \+= with \*= in walk/);
});

test("a fast shard's suite says nothing about a slow shard's timeouts", () => {
  // Shards run on different runners. The 2026-09-25 run had fast shards with MISSED mutants
  // beside slow ones that reported only TIMEOUTs, every one a suite cut short.
  const { gather, current } = run(
    [[outcome("MissedMutant", MISSED, 100)], [outcome("Timeout", HUNG, 240)]],
    ["crates/charter-core/src/a.rs: replace f -> bool with true"],
  );
  const result = gather("--timeout", "240");
  assert.equal(result.code, 1, result.out);
  assert.doesNotMatch(result.out, /## \d+ hang/);
  assert.match(result.out, /\| Timeout \| `crates\/charter-core\/src\/b\.rs` \|/);
  assert.match(current(), /b\.rs: replace \+= with \*= in walk/);
});

test("without the run's timeout, a timeout is a survivor", () => {
  const { gather } = run(
    [[outcome("MissedMutant", MISSED, 20)], [outcome("Timeout", HUNG, 240)]],
    ["crates/charter-core/src/a.rs: replace f -> bool with true"],
  );
  assert.equal(gather().code, 1);
});

// ------------------------------------------------------------------------------------------ //
// the nightly's one issue                                                                    //
// ------------------------------------------------------------------------------------------ //

const TITLE = "nightly mutation testing is not clean";
const RUN_URL = "https://github.com/diazoxide/charter/actions/runs/1";

// `notice` reads the open issues as `gh api --paginate` prints them: one JSON object a line.
function notice(jobs, issues) {
  const root = mkdtempSync(join(tmpdir(), "charter-notice-"));
  const body = join(root, "body.md");
  const result = report(
    [
      "notice",
      "--title",
      TITLE,
      "--run-url",
      RUN_URL,
      "--plan",
      jobs.plan ?? "success",
      "--scope",
      jobs.scope ?? "diff",
      "--baseline",
      jobs.baseline ?? "success",
      "--core",
      jobs.core ?? "success",
      "--survivors",
      jobs.survivors ?? "success",
      "--body-file",
      body,
    ],
    { input: issues.map((issue) => JSON.stringify(issue)).join("\n") },
  );
  assert.equal(result.code, 0, result.out + result.err);
  let text = "";
  try {
    text = readFileSync(body, "utf8");
  } catch {
    // Nothing to say, so nothing written.
  }
  return { actions: result.out.trim().split("\n").filter(Boolean), body: text };
}

// More open issues than one page of `gh issue list` holds, ours the oldest: the shape that
// filed #973 and #1140 as copies of #480.
function backlog(...ours) {
  const others = Array.from({ length: 300 }, (_, i) => ({ number: 2000 - i, title: `work ${i}` }));
  return [...others, ...ours.map((number) => ({ number, title: TITLE }))];
}

test("a red night updates the nightly's issue wherever it sits among the open ones", () => {
  const { actions, body } = notice({ baseline: "failure" }, backlog(480));
  assert.deepEqual(actions, ["edit 480"]);
  assert.match(body, new RegExp(`Last run: ${RUN_URL}`));
  assert.match(body, /charter-core's own tests do not pass/);
});

test("a red night with no issue open files one", () => {
  const { actions, body } = notice({ survivors: "failure" }, backlog());
  assert.deepEqual(actions, ["create"]);
  assert.match(body, /surviving mutants that were not there before/);
});

test("copies of the issue are folded into the oldest one", () => {
  const { actions } = notice({ core: "failure" }, backlog(1140, 480, 973));
  assert.deepEqual(actions, ["edit 480", "duplicate 973 480", "duplicate 1140 480"]);
});

test("only a clean full run closes the issue, and every copy of it", () => {
  assert.deepEqual(notice({ scope: "full" }, backlog(480, 1140)).actions, [
    "close 480",
    "close 1140",
  ]);
  assert.deepEqual(notice({ scope: "diff" }, backlog(480)).actions, []);
});

test("a run that is not clean in any job is not a clean night", () => {
  for (const job of ["plan", "baseline", "core", "survivors"]) {
    const { actions } = notice({ scope: "full", [job]: "cancelled" }, backlog(480));
    assert.deepEqual(actions, ["edit 480"], job);
  }
});

test("a night with nothing to test leaves the issue alone", () => {
  assert.deepEqual(notice({ scope: "none" }, backlog(480)).actions, []);
});

test("an issue that only contains the title is not the nightly's", () => {
  const { actions } = notice({ core: "failure" }, [{ number: 7, title: `Re: ${TITLE}` }]);
  assert.deepEqual(actions, ["create"]);
});
