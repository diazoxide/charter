import { readFileSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * Every launcher in this repository starts the app inside a plane of the run's own, and says
 * so in a way this file can read.
 *
 * **charter-app#129.** The app writes to the plane it resolves, and `charter-app` is checked
 * out at `workspaces/ide/charter-app` — *inside* the operator's control plane. A launcher
 * that pins no plane therefore resolves the operator's: 49 of the scenario suite's chats
 * ended up in `/Users/aharon/IdeaProjects/charter/.charter/app/reopen.json`, where the app's
 * next launch started every one of them. `wdio.bench.conf.ts` was the file that had it
 * worst — it spread the scenario config and then replaced `services` whole, dropping
 * `CHARTER_ROOT` and `CHARTER_CONFIG_HOME` with it, silently, for months.
 *
 * **Why a test and not just the fence.** The fence in `charter-core` makes an unpinned run
 * *die* rather than write, which is the guard that cannot be refactored away. This is the
 * other half: it names the thing that puts the fence there. A config could pass the fence by
 * simply never starting an app, and the app's `e2e` cargo feature — which is what compiles
 * the fence into the binary the scenario tests drive — is three files away from anything a
 * scenario author reads. Both halves are asserted here so that removing either one is red.
 */
const here = dirname(fileURLToPath(import.meta.url));

/** The service options each launcher hands WebdriverIO, which is where the environment is. */
async function serviceEnvironmentOf(config: string): Promise<Record<string, string>> {
  const loaded = (await import(join(here, config))) as {
    config: { services: [string, { env?: Record<string, string> }][] };
  };
  const [, options] = loaded.config.services[0];
  return options.env ?? {};
}

/** Whether `path` is at `tree` or below it, by path components and never by string prefix. */
function inside(path: string, tree: string): boolean {
  const step = relative(resolve(tree), resolve(path));
  return step === "" || (!step.startsWith(`..${sep}`) && step !== ".." && !step.startsWith(sep));
}

const launchers = ["wdio.conf.js", "wdio.state.conf.js", "wdio.bench.conf.js"];

describe("every launcher starts the app inside a plane the run made", () => {
  it.each(launchers)("%s pins the plane, its machine store and the fence", async (config) => {
    const env = await serviceEnvironmentOf(config);

    // The plane: without it the app walks up from its working directory and finds whichever
    // plane the checkout sits in.
    expect(env.CHARTER_ROOT, `${config} starts the app in no plane of its own`).toBeTruthy();
    // The machine store: what this machine remembers and what the operator has approved.
    // Shared with the runner's own, a run writes approvals into the operator's config.
    expect(
      env.CHARTER_CONFIG_HOME,
      `${config} lets the run write the runner's own machine store`,
    ).toBeTruthy();
    // The fence: what makes the two above a failure rather than a silence when they go.
    expect(
      env.CHARTER_PLANE_FENCE,
      `${config} starts an unfenced app, so a plane it must not touch is a write and not a crash`,
    ).toBeTruthy();
  });

  it.each(launchers)("%s pins a plane that is inside its own fence", async (config) => {
    const env = await serviceEnvironmentOf(config);

    expect(
      inside(env.CHARTER_ROOT, env.CHARTER_PLANE_FENCE),
      `${config} pins ${env.CHARTER_ROOT}, which its own fence ${env.CHARTER_PLANE_FENCE} ` +
        `does not hold — the app would die on the plane the run means`,
    ).toBe(true);
  });

  it.each(launchers)(
    "%s gives the run a machine store of its own, not a shared one",
    async (config) => {
      const env = await serviceEnvironmentOf(config);

      expect(inside(env.CHARTER_CONFIG_HOME, env.CHARTER_PLANE_FENCE)).toBe(true);
    },
  );

  it("the app the scenario tests drive is built with the fence compiled into it", () => {
    // `e2e` is the feature the scenario build turns on (`tauri.e2e.conf.json`), and it is
    // where `charter-core/fenced` has to hang: nothing else in the app's graph turns the
    // fence on for a binary, because `[dev-dependencies]` reaches `cargo test` and not
    // `tauri build`. Dropped from here, every assertion above still passes and the scenario
    // suite silently goes back to being able to write the operator's plane.
    const manifest = readFileSync(join(here, "..", "src-tauri", "Cargo.toml"), "utf8");
    const feature = /^e2e = \[(.*)\]$/m.exec(manifest);

    expect(feature, "the app has no e2e feature any more").not.toBeNull();
    expect(feature?.[1]).toContain("charter-core/fenced");
  });
});
