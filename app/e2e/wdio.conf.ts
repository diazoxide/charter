import process from "node:process";
import { built, writeShell } from "./harness.js";

/**
 * The scenario tests: WebdriverIO driving the real app, with the fake harness standing in for
 * Claude Code or Codex.
 *
 * The app is driven through the WebDriver server inside it (`driverProvider: "embedded"`,
 * the service's default), which is the only path that works on macOS as well as Linux — see
 * the service's ADR 0002. Both plugins are behind the app's `e2e` cargo feature.
 */
const app = built(process.platform === "win32" ? "charter-app.exe" : "charter-app");

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: ["./specs/**/*.e2e.ts"],
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
        env: { SHELL: writeShell(built("fake-harness")) },
      },
    ],
  ],
};
