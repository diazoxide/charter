import process from "node:process";
import { built, copyFixturePlane, declareAProfile, writeShell } from "./harness.js";

/**
 * The scenario tests: WebdriverIO driving the real app, with the fake harness standing in for
 * Claude Code or Codex.
 *
 * A build made with `src-tauri/tauri.e2e.conf.json` has its OWN app identifier, so the
 * single-instance plugin does not confuse it with a charter the operator happens to be
 * running. Sharing one made a dev app silently break every scenario run: the app under test
 * handed itself off to the other one and exited 0, and the service reported only "the app
 * likely crashed during startup".
 *
 * The app is driven through the WebDriver server inside it (`driverProvider: "embedded"`,
 * the service's default), which is the only path that works on macOS as well as Linux — see
 * the service's ADR 0002. Both plugins are behind the app's `e2e` cargo feature.
 */
const app = built(process.platform === "win32" ? "charter-app.exe" : "charter-app");

// The plane the app is started in: a fresh copy of a fixture plane, never the committed one.
const plane = copyFixturePlane();
// The plane declares one harness profile, which is what the picker picks from. It is
// deliberately NOT approved: approving it is the operator's click, and the scenario makes it.
declareAProfile(plane, writeShell(built("fake-harness")));

export const config: WebdriverIO.Config = {
  runner: "local",
  // The state specs run separately, against a harness that reports through hooks
  // (`wdio.state.conf.ts`): the app reads `SHELL` once, and one run is one harness.
  specs: ["./specs/**/*.e2e.ts"],
  exclude: ["./specs/**/*.state.e2e.ts"],
  maxInstances: 1,
  framework: "mocha",
  reporters: ["spec"],
  logLevel: "warn",
  waitforTimeout: 20_000,
  connectionRetryTimeout: 120_000,
  mochaOpts: { ui: "bdd", timeout: 180_000 },

  // The service reads `tauri:options`, which WebdriverIO's own capability type does not know.
  capabilities: [{ browserName: "tauri", "tauri:options": { application: app } }] as never,
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: app,
        // The app's own output, for when a scenario test fails because the app did.
        captureBackendLogs: true,
        captureFrontendLogs: true,
        // Every session the app opens is the fake harness, because that is the shell it finds.
        env: { SHELL: writeShell(built("fake-harness")), CHARTER_ROOT: plane },
      },
    ],
  ],
};
