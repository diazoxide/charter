import process from "node:process";
import { built } from "./harness.js";
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

export const config: WebdriverIO.Config = {
  ...scenarios,
  specs: ["./bench/*.bench.ts"],
  reporters: ["spec"],
  mochaOpts: { ui: "bdd", timeout: 900_000 },
  capabilities: [{ browserName: "tauri", "tauri:options": { application: app } }] as never,
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: app,
        captureBackendLogs: true,
        captureFrontendLogs: false,
        env: { SHELL: writeBenchShell(built("fake-harness")) },
      },
    ],
  ],
};
