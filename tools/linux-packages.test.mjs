// The Linux jobs' package install, cached and bounded (#1477). Run with `node --test tools/`.
//
// The script runs against stand-ins for `apt-get`, `timeout` and `dpkg-query` put first on
// PATH, and with no `sudo`, so what it would ask of the mirror is read from a log and never
// sent anywhere.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const script = join(dirname(fileURLToPath(import.meta.url)), "linux-packages.sh");

// A machine to run the script on: a bin/ of stand-ins, an archives/ apt downloads into, a log
// of every apt-get call, and the files GitHub hands a step for its outputs and its summary.
function machine({ failUpdates = 0, failInstalls = 0, failOffline = false } = {}) {
  const root = mkdtempSync(join(tmpdir(), "linux-packages-"));
  const bin = join(root, "bin");
  const archives = join(root, "archives");
  mkdirSync(bin);
  mkdirSync(archives);
  const log = join(root, "apt.log");
  const write = (name, body) => {
    writeFileSync(join(bin, name), `#!/bin/sh\n${body}\n`);
    chmodSync(join(bin, name), 0o755);
  };
  // `timeout -k 10 SECONDS COMMAND...`: the limit is logged, and the command runs.
  write("timeout", `echo "timeout $3" >> "${log}"; shift 3; exec "$@"`);
  // A count per kind of call, so the Nth update or install can be made to time out (124).
  write(
    "apt-get",
    `echo "apt-get $*" >> "${log}"
kind=install
case "$*" in *update*) kind=update ;; *--no-download*) kind=offline ;; esac
n=$(cat "${root}/$kind.n" 2>/dev/null || echo 0); n=$((n + 1)); echo $n > "${root}/$kind.n"
case $kind in
  update) [ $n -le ${failUpdates} ] && exit 124 ;;
  offline) [ ${failOffline ? 1 : 0} = 1 ] && exit 100 ;;
  install) [ $n -le ${failInstalls} ] && exit 124
    touch "${archives}/libfoo_1.0-1_amd64.deb" "${archives}/libbar_1%3a2.0_amd64.deb" "${archives}/stale_0.1_amd64.deb" ;;
esac
exit 0`,
  );
  // What is installed: the two the install brought, and not the stale one.
  write("dpkg-query", `printf 'libfoo_1.0-1_amd64\\nlibbar_1:2.0_amd64\\n'`);
  const output = join(root, "output");
  const summary = join(root, "summary");
  writeFileSync(output, "");
  writeFileSync(summary, "");
  const run = (...args) => {
    const r = spawnSync("bash", [script, ...args], {
      encoding: "utf8",
      env: {
        PATH: `${bin}:${process.env.PATH}`,
        HOME: root,
        GITHUB_OUTPUT: output,
        GITHUB_STEP_SUMMARY: summary,
        ImageOS: "ubuntu24",
        ImageVersion: "20261005.1",
        LINUX_PACKAGES_SUDO: "",
        LINUX_PACKAGES_ARCHIVES: archives,
        LINUX_PACKAGES_PAUSE: "0",
      },
    });
    return {
      ...r,
      apt: existsSync(log) ? readFileSync(log, "utf8").trim().split("\n") : [],
      output: readFileSync(output, "utf8"),
      summary: readFileSync(summary, "utf8"),
    };
  };
  return { root, archives, run };
}

// The step output file is appended to, as GitHub's is, so the last key= line is this run's.
const keyOf = (r) => [...r.output.matchAll(/^key=(.*)$/gm)].at(-1)?.[1];

test("the cache key is the package list, whatever its order, and the runner image", () => {
  const { run } = machine();
  const a = keyOf(run("key", "socat", "bubblewrap"));
  const b = keyOf(run("key", "bubblewrap", "socat", "socat"));
  const c = keyOf(run("key", "bubblewrap"));
  assert.ok(a, "no key= line in the step's output");
  assert.equal(a, b);
  assert.notEqual(a, c);
  assert.match(a, /ubuntu24-20261005\.1/);
});

test("--tauri names the same list as writing Tauri's packages out", () => {
  const { run } = machine();
  const tauri = keyOf(run("key", "--tauri", "socat"));
  const written = keyOf(
    run(
      "key",
      "socat",
      "libwebkit2gtk-4.1-dev",
      "libxdo-dev",
      "libssl-dev",
      "libayatana-appindicator3-dev",
      "librsvg2-dev",
    ),
  );
  assert.equal(tauri, written);
});

test("a first install from the mirror is bounded, keeps what it downloaded, and says how long it took", () => {
  const { root, run } = machine();
  const cache = join(root, "cache");
  const r = run("install", "--cache", cache, "socat");
  assert.equal(r.status, 0, r.stderr);
  assert.ok(r.apt.some((l) => l.startsWith("apt-get") && l.includes("update")));
  assert.ok(r.apt.some((l) => l.startsWith("apt-get") && l.includes("install") && l.endsWith("socat")));
  assert.ok(r.apt.filter((l) => l.startsWith("timeout")).length >= 2, "every apt-get runs under a timeout");
  assert.match(r.output, /^save=true$/m);
  assert.deepEqual(readdirSync(cache).sort(), ["libbar_1%3a2.0_amd64.deb", "libfoo_1.0-1_amd64.deb"]);
  assert.match(r.summary, /from the mirror/);
  assert.match(r.summary, /\d+ s/);
});

test("with the cache restored, the install never asks the mirror", () => {
  const { root, run } = machine();
  const cache = join(root, "cache");
  mkdirSync(cache);
  writeFileSync(join(cache, "libfoo_1.0-1_amd64.deb"), "");
  const r = run("install", "--cache", cache, "socat");
  assert.equal(r.status, 0, r.stderr);
  const apt = r.apt.filter((l) => l.startsWith("apt-get"));
  assert.equal(apt.length, 1, apt.join("\n"));
  assert.match(apt[0], /--no-download/);
  assert.match(apt[0], /libfoo_1\.0-1_amd64\.deb/);
  assert.doesNotMatch(r.output, /^save=true$/m);
  assert.match(r.summary, /from the cache/);
});

test("a cache the image no longer fits falls back to the mirror", () => {
  const { root, run } = machine({ failOffline: true });
  const cache = join(root, "cache");
  mkdirSync(cache);
  writeFileSync(join(cache, "libfoo_1.0-1_amd64.deb"), "");
  const r = run("install", "--cache", cache, "socat");
  assert.equal(r.status, 0, r.stderr);
  assert.ok(r.apt.some((l) => l.includes("update")));
  assert.match(r.stdout + r.stderr, /::warning/);
  assert.match(r.summary, /from the mirror/);
});

test("a mirror that hangs once is tried once more", () => {
  const { run } = machine({ failUpdates: 1 });
  const r = run("install", "socat");
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.apt.filter((l) => l.startsWith("apt-get") && l.includes("update")).length, 2);
  assert.match(r.summary, /attempt 2 of 2/);
});

test("a mirror that hangs twice fails the step with a message saying so", () => {
  const { run } = machine({ failInstalls: 2 });
  const r = run("install", "socat");
  assert.notEqual(r.status, 0);
  assert.equal(r.apt.filter((l) => l.startsWith("apt-get") && l.includes("update")).length, 2);
  assert.match(r.stdout + r.stderr, /::error[^\n]*mirror/);
  assert.match(r.summary, /failed/);
});

test("without --cache nothing is kept for a cache and nothing is read from one", () => {
  const { run } = machine();
  const r = run("install", "--tauri");
  assert.equal(r.status, 0, r.stderr);
  assert.doesNotMatch(r.output, /save=/);
  assert.ok(r.apt.every((l) => !l.includes("--no-download")));
  assert.ok(r.apt.some((l) => l.includes("libwebkit2gtk-4.1-dev")));
});
