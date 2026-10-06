import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { browser, expect } from "@wdio/globals";
import {
  git,
  OLD_CHANNEL,
  OLD_SECRET,
  OLD_SERVICE,
  OLD_VAULT,
  OLD_WORKSPACE_PIN,
} from "../upgrade.js";
import { OLD } from "../wdio.upgrade.conf.js";

/**
 * **An upgrade over an old charter install** (RN-10, #1268): the real app, launched once over a
 * config home under `charter/` and a project with `.charter/`, `charter.toml` and a keyring
 * vault under `charter/…` (`wdio.upgrade.conf.ts` builds them before the launch).
 *
 * What only this can show: the launch's own migration (RN-5, RN-6) running in the built app,
 * before any project opens, and everything the person had still being theirs once it has —
 * the approval, the pins, the channel, the vault — with nothing for the keychain to ask. Then
 * the one step the launch never takes, because it is a commit every teammate pulls: the
 * doctor's `rename-plane`, which runs by name (RN-7) and is offered by no row yet.
 *
 * **It reads, and only the last test writes**: `rename-plane` commits in the project, which is
 * this run's own copy. The run's config removes the old install once it has passed.
 */

/** What the app answered a command with, insisting it answered at all. */
async function ask<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const answer = await browser.executeAsync(
    (
      name: string,
      passed: Record<string, unknown>,
      done: (out: { ok?: unknown; trouble?: string }) => void,
    ) => {
      void window.__TAURI__.core
        .invoke(name, passed)
        .then((ok) => done({ ok }))
        .catch((e: unknown) => done({ trouble: String(e) }));
    },
    command,
    args,
  );
  if (answer.trouble !== undefined) throw new Error(`${command} refused: ${answer.trouble}`);
  return answer.ok as T;
}

/** The project the app opened at launch, once it has: by then the migration has run. */
async function theProject(): Promise<string> {
  let open: string[] = [];
  await browser.waitUntil(
    async () => {
      try {
        open = await ask<string[]>("open_planes");
      } catch {
        return false;
      }
      return open.length > 0;
    },
    { timeout: 60_000, timeoutMsg: "the app never opened the old install's project" },
  );
  return open[0];
}

type MachineProject = {
  path: string;
  approved: boolean;
  pinned: boolean;
  workspace_pins: { name: string; gone: boolean }[];
};

type Fixed = { fix: string; refused: string | null; said: string[]; complete: boolean };

const configHome = (name: string) => join(OLD.configRoot, name);

describe("an upgrade over an old charter install", function () {
  this.timeout(180_000);

  let plane = "";

  before(async () => {
    plane = await theProject();
  });

  it("opened the old install's project", () => {
    expect(plane).toBe(OLD.root);
  });

  it("moved the config home to purlis/, journalled, with no charter/ left beside it", () => {
    expect(existsSync(join(configHome("purlis"), "machine.json"))).toBe(true);
    expect(existsSync(configHome("charter"))).toBe(false);
    expect(existsSync(join(configHome("purlis"), "rename-local", "journal.jsonl"))).toBe(true);
  });

  it("kept the approval, the pins and the channel", async () => {
    const machine = await ask<{ projects: MachineProject[]; dropped: string[] }>("this_machine");
    const project = machine.projects.find((one) => one.path === OLD.root);
    if (!project) throw new Error(`the old project is not remembered: ${JSON.stringify(machine)}`);
    // Approved, and no launch approves a project on its own: only the operator's click does.
    expect(project.approved).toBe(true);
    // Pinned, which nothing but the operator does either.
    expect(project.pinned).toBe(true);
    expect(project.workspace_pins.map((pin) => pin.name)).toContain(OLD_WORKSPACE_PIN);
    expect(machine.dropped).toEqual([]);
    // A store that was lost reads as `stable`, the default.
    expect(await ask<string>("update_channel")).toBe(OLD_CHANNEL);
  });

  it("moved the project's state and local settings to their purlis names", () => {
    expect(existsSync(join(OLD.plane, ".purlis"))).toBe(true);
    expect(existsSync(join(OLD.plane, ".charter"))).toBe(false);
    expect(existsSync(join(OLD.plane, "purlis.local.toml"))).toBe(true);
    expect(existsSync(join(OLD.plane, "charter.local.toml"))).toBe(false);
  });

  it("reads the keyring vault under purlis/, and the keychain had nothing to ask", async () => {
    // Nothing waits for the person's press, which is what an item that would have asked does.
    expect(await ask<unknown>("vaults_to_move")).toBeNull();
    expect(
      await ask<string>("vault_secret_reveal", { plane, vault: OLD_VAULT, key: OLD_SECRET.key }),
    ).toBe(OLD_SECRET.value);

    const state = join(OLD.plane, ".purlis");
    const index = JSON.parse(
      readFileSync(join(state, "vaults", `${OLD_VAULT}.keys.json`), "utf8"),
    ) as { service: string };
    const moved = OLD_SERVICE.replace(/^charter\//, "purlis/");
    expect(index.service).toBe(moved);
    // The stub keychain, never a real one: the copy beside the original, which is kept.
    const stub = JSON.parse(readFileSync(join(state, "keyring-stub.json"), "utf8")) as Record<
      string,
      string
    >;
    expect(stub[`${moved}\n${OLD_SECRET.key}`]).toBe(OLD_SECRET.value);
    expect(stub[`${OLD_SERVICE}\n${OLD_SECRET.key}`]).toBe(OLD_SECRET.value);
  });

  it("runs the doctor's rename-plane by name, in one commit", async () => {
    // The launch never commits; the project's files are still under their old names, and the
    // tree is as clean as the old install left it.
    expect(existsSync(join(OLD.plane, "charter.toml"))).toBe(true);
    expect(git(OLD.plane, ["status", "--porcelain"])).toBe("");
    const before = git(OLD.plane, ["rev-list", "--count", "HEAD"]).trim();

    const fixed = await ask<Fixed>("plane_doctor_fix", { plane, fix: "rename-plane" });

    if (fixed.refused !== null || !fixed.complete) {
      throw new Error(`rename-plane did not run whole: ${JSON.stringify(fixed)}`);
    }
    expect(existsSync(join(OLD.plane, "purlis.toml"))).toBe(true);
    expect(existsSync(join(OLD.plane, "charter.toml"))).toBe(false);
    expect(git(OLD.plane, ["rev-list", "--count", "HEAD"]).trim()).toBe(String(Number(before) + 1));
    expect(git(OLD.plane, ["status", "--porcelain"])).toBe("");
  });
});
