import process from "node:process";
import { anEmptyRecord, built, copyFixturePlane, theRunsEnvironment } from "./harness.js";
import { config as scenarios } from "./wdio.conf.js";
import { writeBenchShell } from "./load.js";

/**
 * The benchmark: the same app and driver as the scenario tests, measuring the spec's limits
 * instead of asserting behaviour. Run it through `tools/bench.mjs`, which builds what it needs
 * and adds the measurements that are taken from outside the window.
 *
 * One app serves every spec file in a run, so `tools/bench.mjs` runs each spec file on its
 * own: what one measures is never left over from another.
 */
const app = built(process.platform === "win32" ? "charter-app.exe" : "charter-app");

/**
 * A plane of the benchmark's own — **and this config used to pin none at all.**
 *
 * It spread the scenario config and then replaced `services` whole, which dropped
 * `CHARTER_ROOT` and `CHARTER_CONFIG_HOME` with it: nothing in the file said so, and nothing
 * failed. The app therefore resolved whatever plane the checkout happened to sit inside — in
 * this repository, the operator's own — and measured itself against that plane's chats while
 * writing its own into that plane's reopen record and its trust into that machine's store.
 * That is charter-app#129 in the file that has it worst.
 *
 * **No profile is declared in it, and that is the point rather than an omission.** The
 * benchmark's `next pane` job presses "New tab" and waits for a pane; it cannot answer a
 * picker, and it never had to, because a plane that declares no harness opens the pane
 * straight onto `$SHELL`. `tests/fixtures/planes/daily` carries no `charter.local.toml`, so
 * the copy is that plane — which is also what the app was measuring before, by accident, and
 * what the 49 chats in charter-app#129's record look like: `profile: ""`, every `program`
 * the harness the run put in `$SHELL`.
 */
const plane = copyFixturePlane();
const shell = writeBenchShell(built("fake-harness"));

export const config: WebdriverIO.Config = {
  ...scenarios,
  specs: ["./bench/*.bench.ts"],
  reporters: ["spec"],
  mochaOpts: { ui: "bdd", timeout: 900_000 },
  // The base's hook empties the base's plane. This run has one of its own, and a hook that
  // closed over the wrong path would quietly clean nothing.
  beforeSession() {
    anEmptyRecord(plane);
  },
  capabilities: [{ browserName: "tauri", "tauri:options": { application: app } }] as never,
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: app,
        captureBackendLogs: true,
        captureFrontendLogs: false,
        env: theRunsEnvironment(plane, { SHELL: shell }),
      },
    ],
  ],
};
