#!/usr/bin/env node
// Measures the app against the spec's limits (docs/spec.md, "Limits"), with tmux
// re-measured beside it
// as a reference. Run from the repository root:
//
//   node tools/bench.mjs                 # build, then measure everything, both renderer arms
//   node tools/bench.mjs --skip-build    # measure what is already built
//   node tools/bench.mjs --only window --arms webgl
//   node tools/bench.mjs --skip-build --only coldstart --app target/debug/charter-app --limit 2000 \
//     --ceiling 2500 --warm-up --fresh-profile
//                                        # CI's Linux cold start: one discarded launch, then each
//                                        # launch on a fresh profile; fails when the median is
//                                        # past the limit or two launches are past the ceiling
//                                        # (one past it is reported, not gated)
//   node tools/bench.mjs --skip-build --only host --baseline <main's charter-session-bench>
//                                        # CI's latency gate (SC-16): the session layer with no
//                                        # window, main's build and the change's interleaved;
//                                        # fails on a sustained regression past 20%
//
// Windows open and close on screen while it runs, and each is brought to the front: WebKit
// draws nothing in a covered window, and nothing at all while the display sleeps (which
// `caffeinate` holds off). Leave the machine alone until it is done.
// The numbers land in target/bench/<timestamp>/results.json, and a table is printed.

import { spawn, spawnSync } from "node:child_process";
import net from "node:net";
import {
  chmodSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { cpus, platform, release, tmpdir, totalmem } from "node:os";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";

import { removeProfile } from "./cleanup.mjs";
import { coldStartGate } from "./coldstart-gate.mjs";
import { latencyGate } from "./latency-gate.mjs";

const ROOT = resolve(import.meta.dirname, "..");
const APP = join(ROOT, "app");
const RELEASE = join(ROOT, "target", "release");
const CORPUS = join(ROOT, "fixtures", "corpora", "claude-code-session.raw");
const CORPUS_BYTES = readFileSync(CORPUS).length;

const { values: options } = parseArgs({
  options: {
    "skip-build": { type: "boolean", default: false },
    only: { type: "string", default: "coldstart,coldstart50,hook,tmux,window" },
    arms: { type: "string", default: "dom,webgl" },
    "cold-starts": { type: "string", default: "10" },
    // The binary cold start launches, when it is not the shipped build this script makes.
    app: { type: "string" },
    // Cold start's limit in ms: the run fails when the held launches' median is past it — or,
    // without a ceiling, when any one is.
    limit: { type: "string" },
    // The most a held launch may take, in ms, beside the median's limit. One launch past it is
    // reported, not gated: one noisy sample on a shared runner is not a regression. Two are.
    ceiling: { type: "string" },
    // One launch before the measured ones, reported and not held to the limit: it pays for a
    // disk cold since boot or install (the binary and GTK/WebKitGTK's libraries), which a person
    // pays once, not per launch.
    "warm-up": { type: "boolean", default: false },
    // Each launch on a HOME and XDG directories of its own, so each is a person's first launch:
    // it pays what a new profile pays — Mesa compiling WebKit's shaders into
    // `~/.cache/mesa_shader_cache`, about half a second — instead of reading the last launch's.
    "fresh-profile": { type: "boolean", default: false },
    // The session layer's bench (`charter-session-bench`), when it is not this tree's build.
    "host-bench": { type: "string" },
    // main's build of the same bench: with it, `host` is the relative gate (SC-16), main's
    // build and this one run in turn, and a sustained regression past 20% fails the run.
    baseline: { type: "string" },
    // Interleaved rounds per pass, each one run of each build.
    rounds: { type: "string", default: "5" },
  },
});
const only = new Set(options.only.split(","));
const stamp = new Date().toISOString().replaceAll(":", "-").slice(0, 19);
const OUT = join(ROOT, "target", "bench", stamp);
mkdirSync(OUT, { recursive: true });

function run(command, args, how = {}) {
  console.log(`\n$ ${[command, ...args].join(" ")}`);
  const done = spawnSync(command, args, { stdio: "inherit", ...how });
  if (done.status !== 0) throw new Error(`${command} ${args.join(" ")} failed: ${done.status}`);
}

function text(command, args, how = {}) {
  const done = spawnSync(command, args, { encoding: "utf8", ...how });
  return done.status === 0 ? done.stdout.trim() : undefined;
}

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

function summary(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  return {
    samples: sorted.length,
    p50: sorted[Math.floor(sorted.length / 2)],
    worst: sorted[sorted.length - 1],
    samples_ms: samples.map((one) => Math.round(one * 10) / 10),
  };
}

// ---------------------------------------------------------------------------------------------
// Build: the app as it ships (for cold start), then the same app with the e2e seam, both in
// release, because a debug build's numbers say nothing about what a person would feel.

const SHIPPED = join(ROOT, "target", "bench", "charter-app-shipped");
const COLD_START_APP = options.app ? resolve(options.app) : SHIPPED;

if (!options["skip-build"]) {
  run(
    "cargo",
    [
      "build", "--release", "-p", "fake-harness", "-p", "charter-cli",
      "-p", "charter-session-protocol", "--features", "charter-session-protocol/bench",
    ],
    { cwd: ROOT },
  );
  run("npx", ["tauri", "build", "--no-bundle"], { cwd: APP });
  cpSync(join(RELEASE, "charter-app"), SHIPPED);
  run(
    "npx",
    ["tauri", "build", "--no-bundle", "--features", "e2e", "--config", "src-tauri/tauri.e2e.conf.json"],
    { cwd: APP },
  );
}

// A display that goes to sleep stops WebKit drawing altogether, and every measurement that
// waits for a paint waits forever. An idle one is nearly as bad: this machine's display drops
// its refresh rate, and nothing can draw more often than the display changes — a run measured
// 22 draws a second where the page itself was only getting 26 frames. `caffeinate` keeps the
// display awake (`-d`), the machine awake (`-i`) and the display at the rate it uses when
// someone is there (`-u`), for as long as this runs and no longer: it watches this process.
// `-u` is the assertion that holds the display at the rate it uses when someone is at the
// machine, and it is the one that needs `-t`: without it the assertion lasts five seconds
// (man caffeinate), so a run measured the rest of itself on a display left to idle. `-t` is
// generous and `-w` still ends it with this process.
const AWAKE_SECONDS = 4 * 60 * 60;
const awake =
  platform() === "darwin"
    ? spawn("caffeinate", ["-d", "-i", "-u", "-t", `${AWAKE_SECONDS}`, "-w", `${process.pid}`], {
        stdio: "ignore",
        detached: true,
      })
    : undefined;
awake?.unref();

const results = {
  machine: {
    date: new Date().toISOString(),
    cpu: cpus()[0]?.model,
    cores: cpus().length,
    memoryGb: Math.round(totalmem() / 2 ** 30),
    os: platform() === "darwin" ? `macOS ${text("sw_vers", ["-productVersion"])}` : `${platform()} ${release()}`,
    tmux: text("tmux", ["-V"]),
    node: process.version,
    cargo: text("cargo", ["--version"]),
    charterApp: text("git", ["-C", ROOT, "rev-parse", "--short", "HEAD"]),
    dirty: Boolean(text("git", ["-C", ROOT, "status", "--porcelain"])),
  },
};

// ---------------------------------------------------------------------------------------------
// Cold start: launching the app as it ships, until its window says the first frame is on
// screen (the `first_frame` command, which prints only when CHARTER_BENCH_LOG is set).

/** Brings a process's window in front, if it has one yet. Failure is ordinary: it has not. */
function activate(pid) {
  if (platform() !== "darwin") return;
  spawnSync("osascript", [
    "-e",
    `tell application "System Events" to set frontmost of (first process whose unix id is ${pid}) to true`,
  ]);
}

/**
 * A machine store for the cold-start arm. It holds which projects this machine remembers and
 * which the operator has approved (ADR 0034), and the app writes it on every launch — so a
 * benchmark left to the runner's own would put its throwaway planes into the operator's
 * `~/.config/charter`, and would measure a store the operator's real use keeps growing.
 */
const COLD_START_CONFIG_HOME = join(tmpdir(), "charter-bench-cold-start-config");
// The host's event log (FD-9) goes here, not into the operator's own data home.
const COLD_START_DATA_HOME = join(tmpdir(), "charter-bench-cold-start-data");

/**
 * A plane for cold start to launch in, holding a record of `chats` chats to put back.
 *
 * With none it is a plain directory with a `charter.toml`, which is the empty-plane case.
 * With some, the app starts that many programs in `setup`, before the window — the one
 * thing reopening adds to a launch, and the reason this arm exists.
 */
function planeForColdStart(chats) {
  const cwd = join(tmpdir(), `charter-bench-cold-start-${chats}`);
  rmSync(cwd, { recursive: true, force: true });
  mkdirSync(join(cwd, ".charter", "app"), { recursive: true });
  writeFileSync(join(cwd, "charter.toml"), "");
  const quiet = join(cwd, "quiet");
  writeFileSync(quiet, "#!/bin/sh\nwhile :; do sleep 1; done\n");
  chmodSync(quiet, 0o755);
  writeFileSync(
    join(cwd, ".charter", "app", "reopen.json"),
    JSON.stringify({
      version: 1,
      at: 0,
      chats: Array.from({ length: chats }, (_, n) => ({
        program: quiet,
        args: [],
        cwd,
        name: `bench.${n}`,
        resume: "",
        active: n === 0,
      })),
    }),
  );
  return cwd;
}

/** A HOME and XDG directories of their own, for a launch that is a person's first. */
function freshProfile() {
  const home = mkdtempSync(join(tmpdir(), "charter-bench-profile-"));
  const dirs = {
    HOME: home,
    XDG_CACHE_HOME: join(home, ".cache"),
    XDG_CONFIG_HOME: join(home, ".config"),
    XDG_DATA_HOME: join(home, ".local", "share"),
    XDG_STATE_HOME: join(home, ".local", "state"),
    CHARTER_CONFIG_HOME: join(home, "charter-config"),
  };
  for (const dir of Object.values(dirs)) mkdirSync(dir, { recursive: true });
  return dirs;
}

/** One launch of the app in `cwd`, to its first frame, with `profile` on top of the env. */
async function launchOnce(cwd, profile) {
  const from = performance.now();
  // The plane is PINNED, not left to `cwd` (charter-app#129). charter puts `$CHARTER_ROOT`
  // into every chat it starts and the variable beats the walk, so a benchmark run from
  // inside a charter chat measured the app against the OPERATOR'S plane — reading its
  // reopen record, which is a list of programs the app then starts, and writing its own
  // back. The fence is what makes a future edit that drops this line a dead app.
  const app = spawn(COLD_START_APP, [], {
    cwd,
    env: {
      ...process.env,
      CHARTER_BENCH_LOG: "1",
      CHARTER_ROOT: cwd,
      CHARTER_PLANE_FENCE: tmpdir(),
      CHARTER_CONFIG_HOME: COLD_START_CONFIG_HOME,
      CHARTER_DATA_HOME: COLD_START_DATA_HOME,
      ...profile,
    },
  });
  // A window that comes up behind another draws no frame at all, so it would never reach
  // the frame this is timing. It is brought forward until it has, as launching it from the
  // dock would: the time that takes is part of the measurement.
  const activating = setInterval(() => activate(app.pid), 50);
  const own = await new Promise((resolve, reject) => {
    let seen = "";
    const giveUp = setTimeout(() => reject(new Error("no first frame within 30 s")), 30_000);
    app.stdout.on("data", (chunk) => {
      seen += chunk;
      const found = seen.match(/charter-bench first-frame (\d+)/);
      if (found) {
        clearTimeout(giveUp);
        resolve({ at: performance.now(), appMs: Number(found[1]) });
      }
    });
    app.on("exit", (code) => reject(new Error(`the app exited (${code}) before its first frame`)));
    // What the app says on its way up — a slow start, a portal it did not wait for — is
    // part of the measurement's record, so it is passed through.
    app.stderr.on("data", (chunk) => process.stderr.write(chunk));
  }).finally(() => clearInterval(activating));
  clearInterval(activating);
  // Only the process this started.
  app.removeAllListeners("exit");
  const ended = new Promise((done) => app.once("exit", done));
  app.kill("SIGTERM");
  await ended;
  await sleep(500);
  if (profile) await removeProfile(profile.HOME);
  return { ms: own.at - from, appMs: own.appMs };
}

async function coldStart(chats = 0) {
  const cwd = planeForColdStart(chats);
  const profile = () => (options["fresh-profile"] ? freshProfile() : undefined);
  const warmUp = options["warm-up"] ? await launchOnce(cwd, profile()) : undefined;
  const samples = [];
  for (let n = 0; n < Number(options["cold-starts"]); n++) samples.push(await launchOnce(cwd, profile()));
  return {
    launchToFirstFrame: summary(samples.map((one) => one.ms)),
    processStartToFirstFrameAsTheAppSeesIt: summary(samples.map((one) => one.appMs)),
    // Without a warm-up the first held launch is the one cold on disk.
    firstRunIsColdestOnDisk: warmUp ? undefined : samples[0]?.ms,
    ...(warmUp && { coldDiskWarmUp: { ms: warmUp.ms, appMs: warmUp.appMs } }),
    freshProfileEachLaunch: options["fresh-profile"],
  };
}

// ---------------------------------------------------------------------------------------------
// Hook call: the spec's 50 ms limit, from three sides.
//
//   - `charter hook stop` through the RUST binary, with an app listening: the call the app's
//     own sessions make, and the one the limit is now measured against.
//   - `charter hook pretooluse` through PYTHON charter, in a copy of the fixture plane since
//     that hook writes into its plane: the guard, which is still Python's and is not this
//     milestone's to move. ADR 0026 measured it at 107.6 ms.
//   - the Rust binary's own start (`charter root`), which is the floor either can reach.
//
// The listening socket is a plain `net` server. It never gets a turn on the event loop while
// `spawnSync` is blocking, and that is not a flaw in the measurement: the kernel accepts into
// the listen backlog, and the hook never waits to be read — the harness is not made to wait
// for charter to draw anything. The reports are counted once the samples are in, so a run
// that measured a hook quietly failing to deliver is not reported as a fast one.

function timed(command, args, how, count) {
  const samples = [];
  for (let n = 0; n < count; n++) {
    const from = performance.now();
    const done = spawnSync(command, args, { stdio: ["pipe", "ignore", "ignore"], ...how });
    samples.push(performance.now() - from);
    if (done.status !== 0) throw new Error(`${command} ${args.join(" ")} failed: ${done.status}`);
  }
  return summary(samples);
}

async function rustHookCall() {
  const socket = join(tmpdir(), `charter-bench-hook-${process.pid}.sock`);
  rmSync(socket, { force: true });
  // Connections, not `data` events: a report delivered in two chunks fires `data` twice, so
  // counting those could show 33 with only 32 reports behind it. An independent review
  // pointed that out.
  let taken = 0;
  const app = net.createServer(() => { taken += 1; });
  await new Promise((listening) => app.listen(socket, listening));

  const conversation = "11111111-2222-4333-8444-555555555555";
  const input = JSON.stringify({
    session_id: conversation,
    transcript_path: "",
    cwd: tmpdir(),
    hook_event_name: "Stop",
    stop_hook_active: false,
    last_assistant_message: "pong",
  });
  const how = {
    cwd: tmpdir(),
    input,
    env: {
      ...process.env,
      CHARTER_HOOK_SOCKET: socket,
      CHARTER_CHAT: "7",
      CLAUDE_CODE_SESSION_ID: conversation,
      // This arm deliberately runs in no plane at all, and an INHERITED `$CHARTER_ROOT`
      // would silently give it one — charter puts that variable into every chat it starts,
      // so running the benchmark from inside a chat measured a hook against the operator's
      // own plane (charter-app#129). Empty is how charter spells "unset".
      CHARTER_ROOT: "",
    },
  };
  const rust = join(RELEASE, "charter");
  timed(rust, ["hook", "stop"], how, 3);
  const measured = timed(rust, ["hook", "stop"], how, 30);

  // The samples are in; let the server drain before it is asked what it got.
  await new Promise((done) => setTimeout(done, 300));
  app.close();
  rmSync(socket, { force: true });
  // 33 = the three warm-ups and the thirty samples. Anything less and the numbers above are
  // the cost of a hook that did not arrive, which is not a measurement of anything. This
  // THROWS rather than recording a field: an earlier version computed `everyReportArrived`
  // and then never looked at it, so a run that lost reports still printed a fast number and
  // a green table. An independent review pointed that out too.
  const expected = 33;
  if (taken !== expected) {
    throw new Error(
      `the hook benchmark measured ${expected} calls but the app took only ${taken} reports; ` +
        `the numbers would be the cost of a hook that never arrived`,
    );
  }
  return { command: "charter hook stop", ...measured, reportsTheAppTook: taken };
}

function hookCall() {
  const plane = join(tmpdir(), "charter-bench-plane");
  cpSync(join(ROOT, "tests", "fixtures", "planes", "daily"), plane, { recursive: true, force: true });
  // PINNED, and not left to `cwd`: `$CHARTER_ROOT` beats the walk, charter puts it into every
  // chat it starts, and a benchmark run from inside a chat therefore measured the operator's
  // own plane rather than this copy of the fixture (charter-app#129).
  const env = { ...process.env, CHARTER_ROOT: plane };
  const out = {};
  const python = text("charter", ["--version"]);
  if (python) {
    const input = JSON.stringify({
      session_id: "bench",
      transcript_path: "",
      cwd: plane,
      hook_event_name: "PreToolUse",
      tool_name: "Bash",
      tool_input: { command: "ls" },
    });
    const version = python.match(/charter (\S+?)(\+|\s|$)/)?.[1] ?? "0";
    timed("charter", ["hook", "pretooluse", "--plugin-version", version], { cwd: plane, input, env }, 3);
    out.pythonCharterPretooluse = {
      version: python,
      ...timed("charter", ["hook", "pretooluse", "--plugin-version", version], { cwd: plane, input, env }, 30),
    };
    // The LIKE-FOR-LIKE row, and the one the Rust number should be read against. ADR 0026
    // measured `pretooluse`, which is the guard and does much more work — quoting it beside
    // `charter hook stop` in Rust invites "97 to 1.8" to be read as this port's speedup when
    // it is two different hooks. Python has a `stop` handler (`hooks._HANDLERS`), so the
    // honest comparison was available all along; an independent review measured it first.
    const stopInput = JSON.stringify({
      session_id: "bench",
      transcript_path: "",
      cwd: plane,
      hook_event_name: "Stop",
      stop_hook_active: false,
    });
    timed("charter", ["hook", "stop", "--plugin-version", version], { cwd: plane, input: stopInput, env }, 3);
    out.pythonCharterStop = {
      version: python,
      ...timed("charter", ["hook", "stop", "--plugin-version", version], { cwd: plane, input: stopInput, env }, 30),
    };
  } else {
    out.pythonCharterPretooluse = "no `charter` on PATH";
    out.pythonCharterStop = "no `charter` on PATH";
  }
  const rust = join(RELEASE, "charter");
  timed(rust, ["root"], { cwd: plane, env }, 3);
  out.rustCharterStart = { command: "charter root", ...timed(rust, ["root"], { cwd: plane, env }, 30) };
  return out;
}

// ---------------------------------------------------------------------------------------------
// tmux, as a reference and not a gate: the same corpus and loads, in a 150x42 window with a
// client attached through a pseudo-terminal, so tmux both parses and redraws. Nothing paints
// its output on a screen, so this is tmux's side of the work only. Timed from releasing the
// load until `capture-pane` shows the sentinel, polled as fast as tmux answers.

async function tmuxReference() {
  const socket = `charter-bench-${process.pid}`;
  const tmux = (...args) => spawnSync("tmux", ["-L", socket, "-f", "/dev/null", ...args], { encoding: "utf8" });
  const harness = join(RELEASE, "fake-harness");
  const out = {};
  for (const [name, loops] of [["corpus once (132 KB)", 1], ["2 MB (corpus x16)", 16], ["13 MB (corpus x99)", 99]]) {
    const sentinel = `TMUX-DONE-${loops}`;
    tmux("kill-server");
    const started = tmux(
      "new-session", "-d", "-s", "bench", "-x", "150", "-y", "42",
      `${harness} --corpus ${CORPUS} --loops ${loops} --wait-for-input --sentinel ${sentinel} --interactive`,
    );
    if (started.status !== 0) throw new Error(`tmux would not start: ${started.stderr}`);
    tmux("set", "-g", "window-size", "manual");
    const client = spawn("script", ["-q", "/dev/null", "tmux", "-L", socket, "attach", "-t", "bench"], {
      stdio: ["pipe", "ignore", "ignore"],
    });
    await sleep(1000);

    const from = performance.now();
    tmux("send-keys", "-t", "bench", "Enter");
    let at;
    while (at === undefined) {
      if (tmux("capture-pane", "-p", "-t", "bench").stdout.includes(sentinel)) at = performance.now();
      if (performance.now() - from > 300_000) throw new Error(`tmux never showed ${sentinel}`);
      // A tight loop would spend a core asking tmux what it is showing while tmux is trying
      // to parse, which takes the measurement out of the load and puts it in the asking.
      await sleep(2);
    }
    const ms = at - from;
    out[name] = { ms, bytes: CORPUS_BYTES * loops, megabytesPerSecond: (CORPUS_BYTES * loops) / 1e6 / (ms / 1000) };
    tmux("kill-server");
    client.kill("SIGTERM");
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// The window: the WebdriverIO benchmark specs, once per renderer arm.

/**
 * Refuses to measure the window on a machine whose screen is locked.
 *
 * A locked session is not drawn, so WebKit hands out no animation frames and no terminal ever
 * paints: every measurement that waits for one waits forever, and a frame count reads zero.
 * The chunking spec is the exception — it counts messages, not paints.
 */
function screenIsLocked() {
  if (platform() !== "darwin") return false;
  const root = text("ioreg", ["-n", "Root", "-d1", "-r", "-a"]) ?? "";
  return root.includes("CGSSessionScreenIsLocked");
}

/** The specs that measure a paint, and so need a screen that is drawn. */
const NEEDS_A_PAINT = (spec) => spec !== "chunking.bench.ts";

function windowArm(arm) {
  const found = {};
  const locked = screenIsLocked();
  // One wdio run per spec file: a run keeps one app for all its spec files, and a spec must
  // not measure what the one before it left running.
  for (const spec of readdirSync(join(APP, "e2e", "bench")).filter((name) => name.endsWith(".bench.ts"))) {
    if (locked && NEEDS_A_PAINT(spec)) {
      found[`${spec} skipped`] = "the screen is locked, so nothing in the window draws";
      continue;
    }
    try {
      run("npx", ["wdio", "run", "e2e/wdio.bench.conf.ts", "--spec", `e2e/bench/${spec}`], {
        cwd: APP,
        env: {
          ...process.env,
          CHARTER_TARGET_DIR: RELEASE,
          CHARTER_BENCH_RENDERER: arm,
          CHARTER_BENCH_OUT: OUT,
        },
      });
    } catch (err) {
      // What the spec did record is still read below; the failure is kept beside it.
      found[`${spec} failed`] = String(err);
    }
  }
  for (const file of readdirSync(OUT).filter((name) => name.startsWith(`${arm}-`))) {
    const { spec, results: numbers } = JSON.parse(readFileSync(join(OUT, file), "utf8"));
    found[spec] = numbers;
  }
  return found;
}

// ---------------------------------------------------------------------------------------------
// The session layer, with no window: `charter-session-bench` runs a host and a client over a
// `charterd.sock` of their own and prints each row's median in ms (L1's keystroke under ten
// flooding panes, L3's 2 MB and 13 MB bursts). With `--baseline`, it is CI's relative gate
// (SC-16): main's build and this one, in the same job, round after round in alternating
// order, so the runner's own speed cancels out of the ratio `tools/latency-gate.mjs` judges.

function hostOnce(bench) {
  const done = spawnSync(bench, ["--corpus", CORPUS], { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
  if (done.status !== 0) throw new Error(`${bench} failed: ${done.status}`);
  return Object.fromEntries(JSON.parse(done.stdout).map((row) => [row.name, row]));
}

function hostPass(head, base, rounds) {
  // One run of each, thrown away: the first run of a binary pays for a cold page cache.
  hostOnce(base);
  hostOnce(head);
  const byRow = {};
  for (let round = 0; round < rounds; round++) {
    // Alternating which goes first, so a runner that slows down or speeds up as the job goes
    // on is not read as one build being slower than the other.
    const [first, second] = round % 2 ? [head, base] : [base, head];
    const one = hostOnce(first);
    const other = hostOnce(second);
    const [ofHead, ofBase] = round % 2 ? [one, other] : [other, one];
    for (const [name, row] of Object.entries(ofHead)) {
      byRow[name] ??= [];
      if (ofBase[name]) byRow[name].push({ base: ofBase[name].value, head: row.value });
    }
  }
  return byRow;
}

function hostArm() {
  const head = options["host-bench"] ? resolve(options["host-bench"]) : join(RELEASE, "charter-session-bench");
  const rounds = Number(options.rounds);
  if (!options.baseline) {
    const runs = Array.from({ length: rounds }, () => hostOnce(head));
    return {
      rows: Object.fromEntries(
        Object.keys(runs[0]).map((name) => {
          const values = runs.map((run) => run[name].value).sort((a, b) => a - b);
          return [name, { unit: "ms", value: values[Math.floor(values.length / 2)], runs: values }];
        }),
      ),
    };
  }
  const base = resolve(options.baseline);
  const passes = [hostPass(head, base, rounds)];
  const judge = () =>
    Object.fromEntries(
      Object.keys(passes[0]).map((name) => [name, latencyGate({ passes: passes.map((pass) => pass[name] ?? []) })]),
    );
  let verdicts = judge();
  // A regression is confirmed by a second pass of its own before it fails anything: one pass
  // can land on a slow minute of the runner's, two in a row past 20% is the change.
  if (Object.values(verdicts).some((one) => one.verdict === "confirm")) {
    passes.push(hostPass(head, base, rounds));
    verdicts = judge();
  }
  return { passes, verdicts };
}

/** The gate's verdicts as a table, for the log and for the job's summary page. */
function hostTable({ passes, verdicts }) {
  const lines = [
    "| row | main (ms) | this change (ms) | ratio, median of rounds | main's own spread | verdict |",
    "|---|---|---|---|---|---|",
  ];
  for (const [name, verdict] of Object.entries(verdicts)) {
    const last = passes.at(-1)[name] ?? [];
    const ms = (pick) => last.map((round) => round[pick].toFixed(3)).join(", ") || "—";
    lines.push(
      `| ${name} | ${ms("base")} | ${ms("head")} | ${verdict.ratio ?? "—"} | ${verdict.spread ?? "—"} | ${verdict.verdict}${verdict.unconfirmed ? " (one pass past 20%, not confirmed)" : ""} |`,
    );
  }
  return lines.join("\n");
}

// ---------------------------------------------------------------------------------------------

if (only.has("host")) results.host = hostArm();
if (only.has("coldstart")) results.coldStart = await coldStart();
// The same launch with a record to put back: the app starts one program per chat in
// `setup`, before the window, so reopening lands inside what a person experiences as the
// launch. The spec's scale is fifty live sessions.
if (only.has("coldstart50")) results.coldStartReopening50 = await coldStart(50);

if (only.has("hook")) {
  results.hookCall = hookCall();
  results.hookCall.rustCharterHook = await rustHookCall();
}
if (only.has("tmux")) results.tmux = await tmuxReference();
if (only.has("window")) {
  if (screenIsLocked()) {
    console.log(
      "\nThe screen is locked, so nothing in the window draws: only the measurements that\n" +
        "need no paint will run. Unlock it and run again for the rest.\n",
    );
  }
  results.window = {};
  for (const arm of options.arms.split(",")) {
    results.window[arm] = windowArm(arm);
  }
}

writeFileSync(join(OUT, "results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${JSON.stringify(results, null, 2)}\n\nWritten to ${join(OUT, "results.json")}`);
if (!existsSync(COLD_START_APP) && only.has("coldstart")) console.log("(cold start needs a build first)");
if (results.host?.rows) {
  // main's record (ADR 0086 §1): github-action-benchmark's `customSmallerIsBetter` shape.
  const record = Object.entries(results.host.rows).map(([name, row]) => ({
    name,
    unit: row.unit,
    value: row.value,
    extra: `median of ${row.runs.length} runs: ${row.runs.map((ms) => ms.toFixed(3)).join(", ")} ms`,
  }));
  writeFileSync(join(OUT, "host-bench.json"), `${JSON.stringify(record, null, 2)}\n`);
  console.log(`\nmain's record: ${join(OUT, "host-bench.json")}`);
}
if (results.host?.verdicts) {
  const table = hostTable(results.host);
  console.log(`\nthe session layer against main, the same job (SC-16):\n${table}`);
  if (process.env.GITHUB_STEP_SUMMARY) {
    writeFileSync(
      process.env.GITHUB_STEP_SUMMARY,
      `### Latency against main, measured in this job\n\n${table}\n\n` +
        "Each cell is one run's median. A row fails only when the median of its rounds' ratios is past " +
        "1.20, at least all but one round is past it, and a second pass agrees (`tools/latency-gate.mjs`).\n",
      { flag: "a" },
    );
  }
  const regressed = Object.entries(results.host.verdicts).filter(([, one]) => one.verdict === "regressed");
  for (const [name, one] of regressed) console.log(`::error::${name} is ${Math.round((one.ratio - 1) * 100)}% slower than main, in two passes`);
  if (regressed.length > 0) process.exitCode = 1;
}
if (options.limit && results.coldStart) {
  // Without a ceiling, every held launch, not the p50: with five launches a p50 lets two of them
  // pass the limit unseen, and a person feels each launch. With one, `coldStartGate` says how.
  const limit = Number(options.limit);
  const build = /[/\\]debug[/\\]/.test(COLD_START_APP)
    ? "debug build"
    : /[/\\](release|bench)[/\\]/.test(COLD_START_APP)
      ? "release build"
      : "build of unknown profile";
  const { coldDiskWarmUp: warmUp, freshProfileEachLaunch: fresh } = results.coldStart;
  if (warmUp)
    console.log(
      `\ncold disk, once per boot or install: reported, not gated: ${Math.round(warmUp.ms)} ms`,
    );
  const launches = results.coldStart.launchToFirstFrame.samples_ms;
  const ceiling = options.ceiling ? Number(options.ceiling) : undefined;
  const { met, median, worst, over, ignored } = coldStartGate({ launches, limit, ceiling });
  console.log(
    `cold start (${build}${fresh ? ", a fresh profile each launch" : ""}): ` +
      `${launches.map((ms) => Math.round(ms)).join(", ")} ms`,
  );
  if (ceiling !== undefined)
    console.log(
      `median ${Math.round(median)} ms against a ${limit} ms limit, worst ${Math.round(worst)} ms ` +
        `against a ${ceiling} ms ceiling: ${met ? "met" : "MISSED"}` +
        (ignored.length > 0
          ? ` (1 launch past the ceiling, ${Math.round(ignored[0])} ms, reported, not gated)`
          : over.length > 1
            ? ` (${over.length} launches past the ceiling, at most 1 is let through)`
            : ""),
    );
  else
    console.log(
      `each against a ${limit} ms limit: ${met ? "met" : "MISSED"}` +
        (over.length > 0 ? ` (${over.length} over)` : launches.length === 0 ? " (no launch)" : ""),
    );
  if (!met) process.exitCode = 1;
}
