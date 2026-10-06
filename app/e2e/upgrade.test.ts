import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  realpathSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { THE_RUNS_TREE } from "./harness.js";
import {
  anOldInstall,
  git,
  inside,
  OLD_CHANNEL,
  OLD_SECRET,
  OLD_SERVICE,
  OLD_VAULT,
  OLD_WORKSPACE_PIN,
  removeTheOldInstall,
} from "./upgrade.js";

/**
 * The old install the upgrade spec launches over (RN-10, #1268), checked here because the spec
 * itself first runs in CI: a fixture that was not the old build's layout would make the spec
 * pass or fail for the wrong reason.
 */

const SHELL = "/bin/true";

/** Every name anywhere under `dir`. */
function everyName(dir: string): string[] {
  return readdirSync(dir, { recursive: true, encoding: "utf8" });
}

describe("the old install the upgrade spec launches over", () => {
  it("is refused anywhere but inside the run's tree, and nothing is written", () => {
    const elsewhere = mkdtempSync(join(tmpdir(), "not-the-run-"));
    expect(() => anOldInstall(THE_RUNS_TREE, elsewhere, SHELL)).toThrow(/outside the run's tree/);
    expect(readdirSync(elsewhere)).toEqual([]);
  });

  it("keeps the machine store under charter/, as the old build did", () => {
    const home = mkdtempSync(join(THE_RUNS_TREE, "home-"));
    const old = anOldInstall(THE_RUNS_TREE, home, SHELL);

    expect(old.configRoot).toBe(join(home, ".config"));
    expect(readdirSync(old.configRoot)).toEqual(["charter"]);
    const file = join(old.configRoot, "charter", "machine.json");
    expect(statSync(file).mode & 0o777).toBe(0o600);
    expect(statSync(join(old.configRoot, "charter")).mode & 0o777).toBe(0o700);
    const store = JSON.parse(readFileSync(file, "utf8"));
    expect(store.channel).toBe(OLD_CHANNEL);
    expect(store.recents).toHaveLength(1);
    expect(store.recents[0]).toMatchObject({
      plane: old.root,
      pinned: true,
      pinnedWorkspaces: [OLD_WORKSPACE_PIN],
    });
    expect(store.recents[0].trust.approved).toBeGreaterThan(0);
    // The workspace pinned is one the fixture has.
    expect(existsSync(join(old.plane, "workspaces", OLD_WORKSPACE_PIN))).toBe(true);
  });

  it("holds a project under its old names, with a keyring vault on the stub", () => {
    const home = mkdtempSync(join(THE_RUNS_TREE, "home-"));
    const old = anOldInstall(THE_RUNS_TREE, home, SHELL);

    expect(inside(old.plane, THE_RUNS_TREE)).toBe(true);
    expect(old.root).toBe(realpathSync(old.plane));
    expect(existsSync(join(old.plane, "charter.toml"))).toBe(true);
    expect(readFileSync(join(old.plane, "charter.local.toml"), "utf8")).toContain(
      JSON.stringify(SHELL),
    );
    const state = join(old.plane, ".charter");
    const registry = JSON.parse(readFileSync(join(state, "vaults.json"), "utf8"));
    expect(registry.vaults[OLD_VAULT]).toEqual({ provider: "keyring", persona: null, config: {} });
    // The fixture's own vault is still registered beside it.
    expect(Object.keys(registry.vaults)).toContain("fixture");
    const index = JSON.parse(readFileSync(join(state, "vaults", `${OLD_VAULT}.keys.json`), "utf8"));
    expect(index.service).toBe(OLD_SERVICE);
    expect(Object.keys(index.keys)).toEqual([OLD_SECRET.key]);
    const stub = join(state, "keyring-stub.json");
    expect(statSync(stub).mode & 0o777).toBe(0o600);
    expect(JSON.parse(readFileSync(stub, "utf8"))).toEqual({
      [`${OLD_SERVICE}\n${OLD_SECRET.key}`]: OLD_SECRET.value,
    });
    // Nothing anywhere under a purlis name yet.
    expect(everyName(old.plane).filter((name) => name.includes("purlis"))).toEqual([]);
  });

  it("is committed and clean, so rename-plane has nothing to refuse over", () => {
    const home = mkdtempSync(join(THE_RUNS_TREE, "home-"));
    const old = anOldInstall(THE_RUNS_TREE, home, SHELL);

    expect(git(old.plane, ["status", "--porcelain"])).toBe("");
    expect(git(old.plane, ["rev-list", "--count", "HEAD"]).trim()).toBe("1");
    // The state folder and the local settings are ignored, never committed.
    const tracked = git(old.plane, ["ls-files"]).split("\n");
    expect(tracked).toContain("charter.toml");
    expect(tracked.filter((path) => path.startsWith(".charter/"))).toEqual([]);
    expect(tracked).not.toContain("charter.local.toml");
  });

  it("is taken away whole, and the run's home stays", () => {
    const home = mkdtempSync(join(THE_RUNS_TREE, "home-"));
    const old = anOldInstall(THE_RUNS_TREE, home, SHELL);

    removeTheOldInstall(old);

    expect(existsSync(old.plane)).toBe(false);
    expect(existsSync(old.configRoot)).toBe(false);
    expect(existsSync(home)).toBe(true);
  });
});

describe("the switch that has the scenario build migrate at launch", () => {
  it("is the variable the app reads, set to the one value it takes", async () => {
    // Spelled twice, in the app and in the run's config, and a typo in either is a run whose
    // app quietly never migrates: every assertion of the spec then fails on the old names.
    const here = dirname(fileURLToPath(import.meta.url));
    const lib = readFileSync(join(here, "..", "src-tauri", "src", "lib.rs"), "utf8");
    const { MIGRATES_AT_LAUNCH, config } = await import("./wdio.upgrade.conf.js");
    expect(lib).toContain(`const SCENARIO_MIGRATES: &str = "${MIGRATES_AT_LAUNCH}";`);
    expect(lib).toContain('value == Some("1")');
    const [, options] = config.services?.[0] as [string, { env: Record<string, string> }];
    expect(options.env[MIGRATES_AT_LAUNCH]).toBe("1");
  });

  it("is left off by every other run", async () => {
    const { MIGRATES_AT_LAUNCH } = await import("./wdio.upgrade.conf.js");
    for (const other of ["wdio.conf.js", "wdio.state.conf.js", "wdio.bench.conf.js"]) {
      const { config } = (await import(`./${other}`)) as { config: WebdriverIO.Config };
      const [, options] = config.services?.[0] as [string, { env: Record<string, string> }];
      expect(options.env[MIGRATES_AT_LAUNCH], other).toBeUndefined();
    }
  });
});
