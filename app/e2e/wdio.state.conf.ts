import { config as base } from "./wdio.conf.js";
import {
  anEmptyRecord,
  built,
  copyFixturePlane,
  declareAProfile,
  theRunsEnvironment,
  writeReportingShell,
} from "./harness.js";
import { PANIC_LOG } from "./processes.js";

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
// A plane of its own, with a profile whose program is the REPORTING harness: a chat here
// starts through the picker like any other, and what it runs has to be the harness this run
// is about.
const plane = copyFixturePlane();
const harness = writeReportingShell(built("fake-harness"), built("charter"));
// `codex`, because that is the kind this stand-in behaves like: it reports no pid and no
// conversation id, which is exactly the rule Codex chats are judged by.
declareAProfile(plane, harness, "codex");

export const config: WebdriverIO.Config = {
  ...base,
  specs: ["./specs/**/*.state.e2e.ts"],
  // The base run EXCLUDES these; spreading it would exclude them here too, and a config
  // that finds no specs exits green-ish rather than loudly.
  exclude: [],
  // The base's own hook cleans the base's plane. This run has one of its own.
  beforeSession() {
    anEmptyRecord(plane);
  },
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: built(process.platform === "win32" ? "charter-app.exe" : "charter-app"),
        captureBackendLogs: true,
        captureFrontendLogs: true,
        // Its own machine store as well as its own plane: this run writes to both, and
        // sharing either with the base run or with the runner's home is a test that passes
        // once (see `aConfigHomeOfItsOwn`). `theRunsEnvironment` gives it both, and the
        // fence that makes forgetting either of them a dead app rather than a poisoned plane.
        env: theRunsEnvironment(plane, {
          SHELL: harness,
          CHARTER_PANIC_LOG: PANIC_LOG,
        }),
      },
    ],
  ],
};
