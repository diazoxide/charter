#!/usr/bin/env node
// Measures the app against the spec's limits (charter repo,
// docs/superpowers/specs/2026-09-17-charter-app.md, "Limits"), with tmux re-measured beside it
// as a reference. Run from the repository root:
//
//   node tools/bench.mjs                 # build, then measure everything, both renderer arms
//   node tools/bench.mjs --skip-build    # measure what is already built
//   node tools/bench.mjs --only window --arms webgl
//
// Windows open and close on screen while it runs, and each is brought to the front: WebKit
// draws nothing in a covered window, so leave the machine alone until it is done.
// The numbers land in target/bench/<timestamp>/results.json, and a table is printed.

import { spawn, spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { cpus, platform, release, tmpdir, totalmem } from "node:os";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";

const ROOT = resolve(import.meta.dirname, "..");
const APP = join(ROOT, "app");
const RELEASE = join(ROOT, "target", "release");
const CORPUS = join(ROOT, "fixtures", "corpora", "claude-code-session.raw");
const CORPUS_BYTES = readFileSync(CORPUS).length;

const { values: options } = parseArgs({
  options: {
    "skip-build": { type: "boolean", default: false },
    only: { type: "string", default: "coldstart,hook,tmux,window" },
    arms: { type: "string", default: "dom,webgl" },
    "cold-starts": { type: "string", default: "10" },
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

if (!options["skip-build"]) {
  run("cargo", ["build", "--release", "-p", "fake-harness", "-p", "charter-cli"], { cwd: ROOT });
  run("npx", ["tauri", "build", "--no-bundle"], { cwd: APP });
  cpSync(join(RELEASE, "charter-app"), SHIPPED);
  run(
    "npx",
    ["tauri", "build", "--no-bundle", "--features", "e2e", "--config", "src-tauri/tauri.e2e.conf.json"],
    { cwd: APP },
  );
}

const results = {
  machine: {
    date: new Date().toISOString(),
    cpu: cpus()[0]?.model,
    cores: cpus().length,
    memoryGb: Math.round(totalmem() / 2 ** 30),
    os: platform() === "darwin" ? `macOS ${text("sw_vers", ["-productVersion"])}` : `${platform()} ${release()}`,
    tmux: text("tmux", ["-V"]),
    charterApp: text("git", ["-C", ROOT, "rev-parse", "--short", "HEAD"]),
    dirty: Boolean(text("git", ["-C", ROOT, "status", "--porcelain"])),
  },
};

// ---------------------------------------------------------------------------------------------
// Cold start: launching the app as it ships, until its window says the first frame is on
// screen (the `first_frame` command, which prints only when CHARTER_BENCH_LOG is set).

async function coldStart() {
  const cwd = join(tmpdir(), "charter-bench-cold-start");
  mkdirSync(cwd, { recursive: true });
  const samples = [];
  for (let n = 0; n < Number(options["cold-starts"]); n++) {
    const from = performance.now();
    const app = spawn(SHIPPED, [], { cwd, env: { ...process.env, CHARTER_BENCH_LOG: "1" } });
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
    });
    samples.push({ ms: own.at - from, appMs: own.appMs });
    // Only the process this started.
    app.removeAllListeners("exit");
    const ended = new Promise((done) => app.once("exit", done));
    app.kill("SIGTERM");
    await ended;
    await sleep(500);
  }
  return {
    launchToFirstFrame: summary(samples.map((one) => one.ms)),
    processStartToFirstFrameAsTheAppSeesIt: summary(samples.map((one) => one.appMs)),
    firstRunIsColdestOnDisk: samples[0]?.ms,
  };
}

// ---------------------------------------------------------------------------------------------
// Hook call: what the plugin runs before every Bash tool call, today — Python charter's
// `charter hook pretooluse` — in a copy of the fixture plane, since the hook writes into its
// plane. And the Rust `charter` binary's own start, which is the floor the Rust hook will
// have once M2 and M3 move hooks there.

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

function hookCall() {
  const plane = join(tmpdir(), "charter-bench-plane");
  cpSync(join(ROOT, "tests", "fixtures", "planes", "daily"), plane, { recursive: true, force: true });
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
    timed("charter", ["hook", "pretooluse", "--plugin-version", version], { cwd: plane, input }, 3);
    out.pythonCharterPretooluse = {
      version: python,
      ...timed("charter", ["hook", "pretooluse", "--plugin-version", version], { cwd: plane, input }, 30),
    };
  } else {
    out.pythonCharterPretooluse = "no `charter` on PATH";
  }
  const rust = join(RELEASE, "charter");
  timed(rust, ["root"], { cwd: plane }, 3);
  out.rustCharterStart = { command: "charter root", ...timed(rust, ["root"], { cwd: plane }, 30) };
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

function windowArm(arm) {
  const found = {};
  // One wdio run per spec file: a run keeps one app for all its spec files, and a spec must
  // not measure what the one before it left running.
  for (const spec of readdirSync(join(APP, "e2e", "bench")).filter((name) => name.endsWith(".bench.ts"))) {
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

if (only.has("coldstart")) results.coldStart = await coldStart();
if (only.has("hook")) results.hookCall = hookCall();
if (only.has("tmux")) results.tmux = await tmuxReference();
if (only.has("window")) {
  results.window = {};
  for (const arm of options.arms.split(",")) {
    results.window[arm] = windowArm(arm);
  }
}

writeFileSync(join(OUT, "results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${JSON.stringify(results, null, 2)}\n\nWritten to ${join(OUT, "results.json")}`);
if (!existsSync(SHIPPED) && only.has("coldstart")) console.log("(cold start needs a build first)");
