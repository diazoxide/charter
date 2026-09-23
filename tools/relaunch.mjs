#!/usr/bin/env node
// The half of the lifecycle a scenario test cannot reach: the app ended and started again.
//
// WebdriverIO's Tauri service keeps one app process for every spec file in a run, so a spec
// that quits takes the rest of the run with it. This owns the process instead. Run it from
// the repository root, with the app and `fake-harness` already built:
//
//   node tools/relaunch.mjs
//
// What it proves, end to end and through the real binary: a record on disk brings a chat
// back under `--resume`, and the app writes that record itself, so the second launch reads
// what the first one wrote rather than what a test hand-wrote.

import { spawn } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const APP = join(process.env.CHARTER_TARGET_DIR ?? join(ROOT, "target", "debug"), "charter-app");
/** The conversation the chat is under. The stand-in harness only has to echo it back. */
const CONVERSATION = "11111111-2222-4333-8444-555555555555";
/**
 * How long the app is given to start its chats.
 *
 * It spawns them in `setup`, which Tauri runs inside `run()` — after the window is up — so
 * this covers the whole launch and not just the chat. Generous because a CI runner is slow
 * and a launch that is merely slow should read as slow, not as broken: the marker log says
 * which step took the time.
 */
const PATIENCE = 90_000;

/**
 * A machine store of this run's own. The store holds which projects this machine remembers
 * and which the operator has approved (ADR 0034); left alone, this script would write its
 * throwaway planes into the operator's `~/.config/charter`.
 */
const CONFIG_HOME = mkdtempSync(join(tmpdir(), "charter-relaunch-config-"));

if (!existsSync(APP)) {
  console.error(`no app at ${APP} — build it first (npx tauri build --debug --no-bundle)`);
  process.exit(1);
}

const failures = [];
function check(what, ok, detail = "") {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures.push(what);
}

/** A plane with nothing in it but the file that makes it one. */
function aPlane() {
  const root = mkdtempSync(join(tmpdir(), "charter-relaunch-"));
  writeFileSync(join(root, "charter.toml"), "");
  return root;
}

/**
 * A program called `claude`, so the app treats it as the harness it is standing in for, which
 * writes down the arguments it was given and then waits to be ended.
 */
function aClaude(where, argvFile) {
  const dir = join(where, "bin");
  mkdirSync(dir, { recursive: true });
  const claude = join(dir, "claude");
  writeFileSync(
    claude,
    [
      "#!/bin/sh",
      `printf '%s\\n' "$*" >> ${JSON.stringify(argvFile)}`,
      // Short sleeps, so that if anything ever does outlive its parent it is gone in a
      // second rather than sitting on the machine for ten minutes.
      "while :; do sleep 1; done",
      "",
    ].join("\n"),
  );
  chmodSync(claude, 0o755);
  return claude;
}

/** Runs the app in `plane` until `done()` is true, then ends it and everything it started. */
async function theAppRuns(plane, done) {
  // `CHARTER_LAUNCH_LOG` makes the app say how far up it got. A launch that never reaches
  // its first chat says nothing at all otherwise, which cost three round trips of guessing.
  //
  // **The plane is PINNED and not left to the working directory** (charter-app#129). This
  // spread the environment and set `cwd`, which reads as enough and is not: charter puts
  // `$CHARTER_ROOT` into every chat it starts, so running this from inside a charter chat
  // inherited that chat's plane — and `$CHARTER_ROOT` beats the walk. The app would then
  // read and rewrite the operator's reopen record, which is a list of programs it starts.
  // The fence says which planes this run may touch at all; the store and the panic log keep
  // the rest of the run off the operator's own.
  const app = spawn(APP, [], {
    cwd: plane,
    stdio: ["ignore", "pipe", "pipe"],
    env: {
      ...process.env,
      CHARTER_LAUNCH_LOG: "1",
      CHARTER_ROOT: plane,
      CHARTER_PLANE_FENCE: tmpdir(),
      CHARTER_CONFIG_HOME: CONFIG_HOME,
    },
  });
  let output = "";
  app.stdout.on("data", (chunk) => (output += chunk));
  app.stderr.on("data", (chunk) => (output += chunk));
  try {
    const until = Date.now() + PATIENCE;
    while (Date.now() < until) {
      if (done()) return { ok: true, output };
      if (app.exitCode !== null) return { ok: false, output: `${output}\n(the app exited)` };
      await new Promise((tick) => setTimeout(tick, 100));
    }
    return { ok: false, output };
  } finally {
    // The pid this started, never a name: other things on this machine are called claude.
    app.kill("SIGTERM");
    await new Promise((gone) => {
      app.on("exit", gone);
      setTimeout(gone, 3_000);
    });
  }
}

/** Every line the stand-in harness has written, which is one per time it was started. */
function argvSeen(file) {
  try {
    return readFileSync(file, "utf8").split("\n").filter(Boolean);
  } catch {
    return [];
  }
}

const plane = aPlane();
const argvFile = join(plane, "argv.txt");
const claude = aClaude(plane, argvFile);
const record = join(plane, ".charter", "app", "reopen.json");
/** When the test says the record was written. Nothing asserts on it; it is there because a
 *  record nobody can date is one nobody can debug. */
const STAMPED = Math.floor(Date.now() / 1000) - 60;

// A record as the app itself writes one, holding one chat under a conversation — and one tab
// that holds a view rather than a chat (ADR 0043, as amended), beside it.
mkdirSync(join(plane, ".charter", "app"), { recursive: true });
writeFileSync(
  record,
  JSON.stringify(
    {
      version: 1,
      at: STAMPED,
      chats: [
        {
          program: claude,
          args: [],
          cwd: plane,
          name: "ide.7",
          resume: CONVERSATION,
          active: true,
        },
      ],
      views: [{ view: "persona", key: "steward", title: "steward", at: 1 }],
    },
    null,
    2,
  ),
);

console.log(`a plane at ${plane}`);

const first = await theAppRuns(plane, () => argvSeen(argvFile).length >= 1);
check("the app starts the chat its record holds", first.ok, first.ok ? "" : `the app said:\n${first.output}`);
const started = argvSeen(argvFile);
check(
  "it is started as a resume of the conversation that was recorded",
  started[0]?.includes(`--resume ${CONVERSATION}`),
  `it was given: ${JSON.stringify(started[0] ?? "")}`,
);
check(
  "and under the name the chat had",
  started[0]?.includes("--name ide.7"),
  `it was given: ${JSON.stringify(started[0] ?? "")}`,
);

// The record a launch leaves behind is what the next launch reads, so it has to still hold
// the chat and its conversation. Asserted on content, never on the file's timestamp: a
// window that has loaded reports which chat is in front, and that is a change the app
// rightly writes — so whether the stamp moved depends on how far the UI got, which is not
// something this test should be pinning.
//
// (That the app writes the record when something changes is `chats.rs`'s own tests, where
// the change can be made directly. Nothing here can click.)
const written = JSON.parse(readFileSync(record, "utf8"));
check(
  "the record still holds the chat, so the next launch has something to read",
  written.chats?.length === 1 && written.chats[0]?.name === "ide.7",
  `the record holds ${JSON.stringify(written.chats?.map((c) => c.name))}`,
);
check(
  "and kept the conversation, so the chat can be resumed again",
  written.chats?.[0]?.resume === CONVERSATION,
  `the record says ${JSON.stringify(written.chats?.[0]?.resume)}`,
);

// A view tab starts nothing, so nothing above could see it come back. What a relaunch owes it is
// that the launch neither drops it on the way in nor writes it out of the record on the way out:
// the core holds what the record put back until the window says otherwise, and a window that
// loaded said the same tabs back.
check(
  "the record still holds the view tab, so the next launch puts that back too",
  written.views?.some((view) => view.view === "persona" && view.key === "steward") === true,
  `the record holds ${JSON.stringify(written.views)}`,
);

// The second launch reads what the first one wrote, not what this script hand-wrote.
const second = await theAppRuns(plane, () => argvSeen(argvFile).length >= 2);
check("a second launch brings the chat back again", second.ok, second.ok ? "" : `the app said:\n${second.output}`);
check(
  "still as a resume of the same conversation",
  argvSeen(argvFile)[1]?.includes(`--resume ${CONVERSATION}`),
  `it was given: ${JSON.stringify(argvSeen(argvFile)[1] ?? "")}`,
);

console.log(failures.length === 0 ? "\nall good" : `\n${failures.length} failed`);
process.exit(failures.length === 0 ? 0 : 1);
