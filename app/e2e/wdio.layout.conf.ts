import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";
import { config as base } from "./wdio.conf.js";
import {
  aConfigHomeOfItsOwn,
  anEmptyRecord,
  built,
  copyFixturePlane,
  theRunsEnvironment,
} from "./harness.js";
import { PANIC_LOG } from "./processes.js";

/**
 * The layout the operator wrote, in place **before the app starts** (M6.9).
 *
 * The file is read by the Rust side before the window exists and handed to the page as it is
 * created, so the only honest test of it is a launch that finds it already there. WebdriverIO's
 * Tauri service keeps one app process for a whole run, so this is a run of its own, exactly as
 * `wdio.finder.conf.ts` is: `wdio.conf.ts` excludes the spec by name.
 *
 * **Everything about the arrangement is off its default**, so a window that drew the default
 * first could not pass by coincidence: the explorer is on the right and wider than it starts,
 * the attention region is on the left, and the state bar is put away.
 */
export const THE_LAYOUT = {
  version: 1,
  regions: [
    { id: "explorer", side: "right", order: 0, collapsed: false, size: 30 },
    { id: "aside", side: "left", order: 0, collapsed: false },
    { id: "bottom", side: "bottom", order: 0, collapsed: true },
  ],
};

// Through the environment, and made only if it is not already there: WebdriverIO's worker
// imports this file again, and the spec has to read the SAME file the launcher wrote.
const plane = (process.env.CHARTER_LAYOUT_PLANE ??= copyFixturePlane());
const home = (process.env.CHARTER_LAYOUT_CONFIG_HOME ??= aConfigHomeOfItsOwn());
/** The file, where charter reads it: `$CHARTER_CONFIG_HOME/charter/layout.json`. */
export const LAYOUT_FILE = join(home, "charter", "layout.json");
mkdirSync(join(home, "charter"), { recursive: true, mode: 0o700 });
writeFileSync(LAYOUT_FILE, JSON.stringify(THE_LAYOUT, null, 2) + "\n", { mode: 0o600 });

export const config: WebdriverIO.Config = {
  ...base,
  specs: ["./specs/**/*.layout.e2e.ts"],
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
        // `CHARTER_CONFIG_HOME` named here replaces the fresh one `theRunsEnvironment` would
        // make: this run's store is the one the layout was written into.
        env: theRunsEnvironment(plane, {
          CHARTER_CONFIG_HOME: home,
          CHARTER_PANIC_LOG: PANIC_LOG,
        }),
      },
    ],
  ],
};
