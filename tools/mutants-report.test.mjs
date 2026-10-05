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
// `tested` and `files` are what `plan` wrote: the files this run's mutants come from, and every
// file of the crate that has a mutant. Either may be missing, as when `plan` itself failed.
function notice(jobs, issues, { tested, files } = {}) {
  const root = mkdtempSync(join(tmpdir(), "charter-notice-"));
  const body = join(root, "body.md");
  const lists = [];
  for (const [flag, list] of [
    ["--tested", tested],
    ["--files", files],
  ]) {
    const path = join(root, flag.slice(2));
    if (list) writeFileSync(path, list.map((file) => file + "\n").join(""));
    lists.push(flag, path);
  }
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
      ...lists,
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

test("a clean full run closes the issue, and every copy of it; a clean diff run never does", () => {
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

// ------------------------------------------------------------------------------------------ //
// Sunday's weekly slice                                                                      //
// ------------------------------------------------------------------------------------------ //

// `cargo mutants --list`, one mutant a line: `count` mutants in `file`.
function listing(counts) {
  return Object.entries(counts)
    .flatMap(([file, count]) =>
      Array.from({ length: count }, (_, i) => `${file}:${i + 1}:1: replace f${i} -> bool with true`),
    )
    .join("\n");
}

function slice(week, slices, counts) {
  const result = report(["slice", "--slices", String(slices), "--week", String(week)], {
    input: listing(counts),
  });
  assert.equal(result.code, 0, result.out + result.err);
  return result.out.trim().split("\n").filter(Boolean);
}

test("a week's slice is whole files, cut by mutant count and rotating with the week", () => {
  const counts = { "src/d.rs": 30, "src/a.rs": 10, "src/c.rs": 10, "src/b.rs": 10 };
  // Sixty mutants in three slices of about twenty: a file goes to the slice its middle falls in.
  assert.deepEqual(slice(0, 3, counts), ["src/a.rs", "src/b.rs"]);
  assert.deepEqual(slice(1, 3, counts), ["src/c.rs"]);
  assert.deepEqual(slice(2, 3, counts), ["src/d.rs"]);
  // The fourth week starts the cycle again.
  assert.deepEqual(slice(3, 3, counts), ["src/a.rs", "src/b.rs"]);
  assert.deepEqual(slice(4, 3, counts), ["src/c.rs"]);
});

test("every file falls in exactly one slice of a cycle", () => {
  const counts = Object.fromEntries(
    Array.from({ length: 40 }, (_, i) => [`src/m${String(i).padStart(2, "0")}.rs`, 1 + ((i * 7) % 13)]),
  );
  const seen = [];
  for (let week = 0; week < 6; week++) {
    const files = slice(week, 6, counts);
    assert.ok(files.length > 0, `week ${week} has a slice`);
    seen.push(...files);
  }
  assert.deepEqual(seen.sort(), Object.keys(counts).sort());
});

// ------------------------------------------------------------------------------------------ //
// the issue, slice by slice                                                                  //
// ------------------------------------------------------------------------------------------ //

const CRATE = ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"];

// The issue as the last night left it: the body `notice` wrote, under the nightly's title.
function left(number, body) {
  return [{ number, title: TITLE, body }];
}

test("an issue that does not say which files are clean holds the whole crate", () => {
  // #480 as HY-7 left it: no record of any slice. One clean slice is not the whole crate.
  const { actions, body } = notice(
    { scope: "slice" },
    [{ number: 480, title: TITLE, body: "Last run: an older one" }],
    { tested: ["src/a.rs", "src/b.rs"], files: CRATE },
  );
  assert.deepEqual(actions, ["edit 480"]);
  assert.match(body, /\*\*2 of charter-core's 4 files are not known clean/);
  // What the last red night said stays: a clean slice adds to the record, it does not erase it.
  assert.match(body, /Last run: an older one/);
});

test("a clean slice clears only its own files, and the last of them closes the issue", () => {
  const red = notice({ scope: "slice", survivors: "failure" }, [], {
    tested: ["src/a.rs", "src/b.rs"],
    files: CRATE,
  });
  assert.deepEqual(red.actions, ["create"]);
  assert.match(red.body, /\*\*2 of charter-core's 4 files are not known clean/);

  // Another slice ran clean: nothing it tested was dirty, so nothing changes and it stays open.
  const other = notice({ scope: "slice" }, left(9, red.body), {
    tested: ["src/c.rs", "src/d.rs"],
    files: CRATE,
  });
  assert.deepEqual(other.actions, ["edit 9"]);
  assert.match(other.body, /\*\*2 of charter-core's 4 files/);

  const half = notice({ scope: "slice" }, left(9, red.body), {
    tested: ["src/a.rs"],
    files: CRATE,
  });
  assert.deepEqual(half.actions, ["edit 9"]);
  assert.match(half.body, /\*\*1 of charter-core's 4 files is not known clean/);

  const last = notice({ scope: "slice" }, left(9, half.body), {
    tested: ["src/b.rs"],
    files: CRATE,
  });
  assert.deepEqual(last.actions, ["close 9"]);
});

test("a slice that did not finish, or was cancelled, never closes the issue", () => {
  for (const jobs of [
    { core: "failure" },
    { core: "cancelled" },
    { baseline: "cancelled" },
    { survivors: "cancelled" },
    { plan: "cancelled" },
  ]) {
    // Its own files were the only ones left, and they are still not known clean.
    const red = notice({ scope: "slice", survivors: "failure" }, [], {
      tested: ["src/a.rs"],
      files: CRATE,
    });
    const { actions, body } = notice({ scope: "slice", ...jobs }, left(9, red.body), {
      tested: ["src/a.rs"],
      files: CRATE,
    });
    assert.deepEqual(actions, ["edit 9"], JSON.stringify(jobs));
    assert.match(body, /not known clean/, JSON.stringify(jobs));
  }
});

test("a clean slice whose list of files is missing clears nothing", () => {
  const red = notice({ scope: "slice", survivors: "failure" }, [], {
    tested: ["src/a.rs"],
    files: CRATE,
  });
  assert.deepEqual(notice({ scope: "slice" }, left(9, red.body), { files: CRATE }).actions, [
    "edit 9",
  ]);
  // Without the crate's list, an issue that holds the whole crate cannot say what is left.
  const whole = left(9, "Last run: an older one");
  assert.deepEqual(notice({ scope: "slice" }, whole, { tested: CRATE }).actions, ["edit 9"]);
});

test("a red diff night marks the files it changed, and their slice running clean closes it", () => {
  const red = notice({ scope: "diff", survivors: "failure" }, [], {
    tested: ["src/c.rs"],
    files: CRATE,
  });
  assert.deepEqual(red.actions, ["create"]);
  assert.match(red.body, /\*\*1 of charter-core's 4 files is not known clean/);
  // A clean diff night after it is not a clean c.rs: it tested only the lines that changed.
  assert.deepEqual(
    notice({ scope: "diff" }, left(9, red.body), { tested: ["src/c.rs"], files: CRATE }).actions,
    [],
  );
  assert.deepEqual(
    notice({ scope: "slice" }, left(9, red.body), { tested: ["src/c.rs", "src/d.rs"], files: CRATE })
      .actions,
    ["close 9"],
  );
});

test("a diff night that did not finish marks the files it changed", () => {
  const first = notice({ scope: "slice", survivors: "failure" }, [], {
    tested: ["src/a.rs"],
    files: CRATE,
  });
  const { actions, body } = notice({ scope: "diff", core: "failure" }, left(9, first.body), {
    tested: ["src/d.rs"],
    files: CRATE,
  });
  assert.deepEqual(actions, ["edit 9"]);
  assert.match(body, /\*\*2 of charter-core's 4 files are not known clean/);
});

test("a night that cannot say what it tested marks the whole crate", () => {
  const first = notice({ scope: "slice", survivors: "failure" }, [], {
    tested: ["src/a.rs"],
    files: CRATE,
  });
  const lost = notice({ plan: "failure" }, left(9, first.body), {});
  assert.deepEqual(lost.actions, ["edit 9"]);
  // The next slice with lists again counts the whole crate, less what it cleared.
  const next = notice({ scope: "slice" }, left(9, lost.body), {
    tested: ["src/a.rs", "src/b.rs"],
    files: CRATE,
  });
  assert.match(next.body, /\*\*2 of charter-core's 4 files are not known clean/);
});

test("a file deleted from the crate does not hold the issue open", () => {
  const red = notice({ scope: "slice", survivors: "failure" }, [], {
    tested: ["src/a.rs", "src/gone.rs"],
    files: [...CRATE, "src/gone.rs"],
  });
  const { actions } = notice({ scope: "slice" }, left(9, red.body), {
    tested: ["src/a.rs"],
    files: CRATE,
  });
  assert.deepEqual(actions, ["close 9"]);
});

test("a red full run marks the whole crate", () => {
  const { actions, body } = notice({ scope: "full", survivors: "failure" }, [], {
    tested: CRATE,
    files: CRATE,
  });
  assert.deepEqual(actions, ["create"]);
  assert.match(body, /\*\*4 of charter-core's 4 files are not known clean/);
});
