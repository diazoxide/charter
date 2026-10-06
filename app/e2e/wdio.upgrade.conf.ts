import process from "node:process";
import { config as base } from "./wdio.conf.js";
import {
  anEmptyRecord,
  built,
  theRunsEnvironment,
  THE_RUNS_HOME,
  THE_RUNS_TREE,
  writeShell,
} from "./harness.js";
import { PANIC_LOG } from "./processes.js";
import { anOldInstall, removeTheOldInstall, type OldInstall } from "./upgrade.js";

/**
 * **The app launched over an old charter install** (RN-10, #1268): its config home under
 * `charter/`, a project with `.charter/`, `charter.toml` and a keyring vault under `charter/…`,
 * all in place **before the app starts**, because the migration under test runs at launch.
 *
 * WebdriverIO's Tauri service keeps one app process for a whole run, so this is a run of its
 * own, as `wdio.layout.conf.ts` is, and `wdio.conf.ts` excludes the spec by name.
 *
 * **The one run whose app migrates at launch.** The scenario build skips the launch's
 * rename-local, because every other spec names the old folders (D-RN5-7);
 * `PURLIS_E2E_RENAME_LOCAL=1` turns it on for this app alone.
 *
 * **Inside the run's own `$HOME`** (`THE_RUNS_HOME`), never the runner's: the config home is
 * `<home>/.config`, named by `$CHARTER_CONFIG_HOME` as well, because a CI runner exports an
 * `$XDG_CONFIG_HOME` of its own. The keychain is the fenced build's stub.
 */

/** The variable that turns the launch migration on in the scenario build (`lib.rs`). */
export const MIGRATES_AT_LAUNCH = "PURLIS_E2E_RENAME_LOCAL";

// Through the environment, and made only once: WebdriverIO's worker imports this file again,
// and a second build there would put `charter/` back after the app had moved it.
const made = process.env.CHARTER_UPGRADE_INSTALL;
export const OLD: OldInstall = made
  ? (JSON.parse(made) as OldInstall)
  : anOldInstall(THE_RUNS_TREE, THE_RUNS_HOME, writeShell(built("fake-harness")));
process.env.CHARTER_UPGRADE_INSTALL = JSON.stringify(OLD);

export const config: WebdriverIO.Config = {
  ...base,
  specs: ["./specs/**/*.upgrade.e2e.ts"],
  // The base run EXCLUDES this spec; spreading its `exclude` would exclude it here too, and a
  // config that finds no specs exits green-ish rather than loudly.
  exclude: [],
  beforeSession() {
    anEmptyRecord(OLD.plane);
  },
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: built(process.platform === "win32" ? "charter-app.exe" : "charter-app"),
        captureBackendLogs: true,
        captureFrontendLogs: true,
        // `CHARTER_CONFIG_HOME` named here replaces the fresh one `theRunsEnvironment` would
        // give: this run's store is the old one. `HOME` is the run's own (`OLD.home`), with
        // its git identity, which `rename-plane`'s commit is made with.
        env: theRunsEnvironment(OLD.plane, {
          CHARTER_CONFIG_HOME: OLD.configRoot,
          SHELL: writeShell(built("fake-harness")),
          CHARTER_PANIC_LOG: PANIC_LOG,
          [MIGRATES_AT_LAUNCH]: "1",
        }),
      },
    ],
  ],
  // Nothing of the old install outlives the run. A failed run keeps it, beside the rest of its
  // evidence, for whoever reads why.
  onComplete(exitCode: number) {
    if (exitCode === 0) removeTheOldInstall(OLD);
  },
};
