import { config as base } from "./wdio.conf.js";
import { built, writeReportingShell } from "./harness.js";

/**
 * The scenario tests for what a chat is doing — a run of their own, because the harness is
 * different.
 *
 * WebdriverIO's Tauri service keeps ONE app process for a whole run, and the app reads its
 * harness from `SHELL` once, at the start. The other specs want a harness that writes its
 * output immediately; these want one that reports through `charter hook` and holds its
 * output until a line is typed, so `running` can be seen without racing the turn's end. Two
 * harnesses is two runs.
 */
export const config: WebdriverIO.Config = {
  ...base,
  specs: ["./specs/**/*.state.e2e.ts"],
  // The base run EXCLUDES these; spreading it would exclude them here too, and a config
  // that finds no specs exits green-ish rather than loudly.
  exclude: [],
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: built(process.platform === "win32" ? "charter-app.exe" : "charter-app"),
        captureBackendLogs: true,
        captureFrontendLogs: true,
        env: { SHELL: writeReportingShell(built("fake-harness"), built("charter")) },
      },
    ],
  ],
};
