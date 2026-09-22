import process from "node:process";
import { config as base } from "./wdio.conf.js";
import {
  A_FINDER_LAUNCHS_PATH,
  aClaudeConfigHomeOfItsOwn,
  anEmptyRecord,
  built,
  copyFixturePlane,
  theRunsEnvironment,
  writeAHarnessOnlyAShellWouldFind,
  writeAPluginHookingShell,
} from "./harness.js";
import { PANIC_LOG } from "./processes.js";

/**
 * The status line this launch's operator already has, which charter may not replace.
 *
 * Exported so the spec asserts against the same string the launcher wrote: a doctor row that
 * named a different file, or no file, would pass a test that compared prose with prose.
 */
export const THEIR_STATUS_LINE = "/bin/echo their own status line";

/**
 * The app started the way an operator starts it: from Finder, with the `PATH` Finder gives.
 *
 * **charter-app#134, and the reason it reached the operator.** macOS hands a double-clicked
 * `.app` `PATH=/usr/bin:/bin:/usr/sbin:/sbin` — `launchd` starts a GUI process and no login
 * shell is involved. The operator's `claude` was in `~/.local/bin`, where its own installer
 * puts it, so charter's wiring probe could not spawn it, answered `State::Unknown`, and the
 * app refused every chat with *"an unknown is not a pass — nothing was started"*.
 *
 * **Nothing in this suite had ever launched the app that way.** Every other config inherits
 * the launcher's environment, so the app under test gets CI's `PATH` — which already holds
 * everything — and every profile a spec uses is declared with an ABSOLUTE command, so no
 * search is ever exercised. Two independent reasons the whole class of defect was invisible,
 * and this config removes both: the environment is Finder's, and the profile is the BUILT-IN
 * `claude`, whose command is the bare word `claude` out of charter's registry.
 *
 * A run of its own, because this is the app's environment and WebdriverIO's Tauri service
 * keeps one app process for a whole run. `wdio.conf.ts` excludes this spec by name.
 */
// Through the environment, and made only if it is not already there: WebdriverIO's worker
// imports this file again, and `launch.finder.e2e.ts` has to read the SAME plane and `$HOME`
// the launcher gave the app — the chat's recorded `PATH` is in one, and charter-app#136's
// `~/.local/bin/charter` goes in the other. A fresh copy per import would be a spec reading a
// tree the app never saw.
const plane = (process.env.CHARTER_FINDER_PLANE ??= copyFixturePlane());
// No `declareAProfile`: the profile under test is charter's own built-in, and declaring one
// would hand the app an absolute path and test nothing. What the plane gets instead is a
// `$HOME` with the harness in it — installed, findable by a shell, invisible to `PATH`.
// A shell script and not `built("fake-harness")`: the fake harness only prints the sentinel a
// spec waits for when it is given `--synthetic/--sentinel/--interactive`, and the profile's
// program drops its own arguments (charter puts `--session-id`/`--name` on a Claude Code
// line), so the flags have to be inside what it execs.
//
// And it runs a hook the way the charter PLUGIN spells one — `charter hook sessionstart`, the
// bare word (charter-app#136). The hooks charter arms on the chat itself name its binary by an
// absolute path and never needed `PATH`; the plugin's and a plane's own cannot, because those
// files travel to other machines. So what this run can see is whether a chat started from
// Finder can find `charter` at all.
const home = (process.env.CHARTER_FINDER_HOME ??= writeAHarnessOnlyAShellWouldFind(
  writeAPluginHookingShell(built("fake-harness")),
));

export const config: WebdriverIO.Config = {
  ...base,
  specs: ["./specs/**/*.finder.e2e.ts"],
  // The base run EXCLUDES this spec; spreading its `exclude` would exclude it here too, and a
  // config that finds no specs exits green-ish rather than loudly.
  exclude: [],
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
        // The service spreads `process.env` and then this, so naming `PATH` and `HOME` here
        // REPLACES the launcher's. That is the point of the file: everything else about the
        // run is ordinary, and the app is given the environment Finder gives.
        //
        // `CHARTER_ROOT`, `CHARTER_CONFIG_HOME` and the fence still come from
        // `theRunsEnvironment` — a minimal `PATH` is not a licence to touch a real plane
        // (charter-app#129).
        env: theRunsEnvironment(plane, {
          PATH: A_FINDER_LAUNCHS_PATH,
          HOME: home,
          CHARTER_PANIC_LOG: PANIC_LOG,
          // **This launch has a status line of the operator's own**, which is the other half
          // of the 2026-09-22 ruling: charter must not replace it, and `doctor` must say why
          // the chat's ctx/cache gauge is dark. `launch.finder.e2e.ts` asks the doctor.
          CLAUDE_CONFIG_DIR: aClaudeConfigHomeOfItsOwn(THEIR_STATUS_LINE),
        }),
      },
    ],
  ],
};
