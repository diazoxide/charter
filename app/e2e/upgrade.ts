import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import process from "node:process";
import { copyFixturePlane } from "./harness.js";

/**
 * **A machine as a charter from before the rename left it** (RN-10, #1268): the config home
 * under `charter/`, a project whose state is in `.charter/` and whose manifest is
 * `charter.toml`, and a keyring vault whose items are under `charter/…`.
 *
 * Written byte for byte as the old build wrote them, never by this build, which writes the
 * purlis names. The keychain is the fenced build's stub (`<state>/keyring-stub.json`), so
 * nothing here, and nothing the app does with it, reaches a real keychain.
 *
 * `wdio.upgrade.conf.ts` builds it before the app starts, and `upgrade.upgrade.e2e.ts` asks
 * what the app's launch migration made of it.
 */

/** The keyring vault the old install holds. */
export const OLD_VAULT = "ops";
/** Its service, as the old build named it: `charter/<vault>/<8 hex>`. */
export const OLD_SERVICE = `charter/${OLD_VAULT}/5e1f0c2a`;
/** The one secret in it. Twenty bytes, so its size band is `16–31 bytes`. */
export const OLD_SECRET = { key: "API_TOKEN", value: "e2e-upgrade-7d41c9b2" } as const;
/** The channel the old install was on: not the default, so a store that was lost reads as one. */
export const OLD_CHANNEL = "dev";
/**
 * The workspace pinned in the project: `beta`, not `alpha`, because a project opened with no
 * pins pins its most active workspace, and the fixture's most active one is `alpha`.
 */
export const OLD_WORKSPACE_PIN = "beta";
/** When the old install approved the project, in seconds since the epoch. */
export const OLD_APPROVED = 1_759_000_000;

/** Where the old install is. */
export interface OldInstall {
  /** The `$HOME` the app is given. */
  home: string;
  /** The config root, `<home>/.config`: what `$CHARTER_CONFIG_HOME` names. */
  configRoot: string;
  /** The project, as it was copied into the run's tree: what `$CHARTER_ROOT` names. */
  plane: string;
  /**
   * The project's real path, which is what the app resolves it to and what the old build
   * remembered: the two differ where the temporary directory is reached through a link, as
   * macOS's `/var` is.
   */
  root: string;
}

/** Whether `path` is `within` or below it. */
export function inside(path: string, within: string): boolean {
  const step = relative(within, path);
  return step === "" || (!step.startsWith(`..${sep}`) && step !== ".." && !step.startsWith(sep));
}

/** Writes `value` as JSON, 0600, as charter writes every file it keeps private. */
function writePrivate(file: string, value: unknown): void {
  mkdirSync(dirname(file), { recursive: true, mode: 0o700 });
  writeFileSync(file, JSON.stringify(value, null, 2) + "\n", { mode: 0o600 });
}

/**
 * git, for the setup only: never the runner's own configuration, which may carry hooks, a
 * default branch or signing that a fixture must not depend on.
 */
export function git(cwd: string, args: string[]): string {
  return execFileSync(
    "git",
    ["-c", "user.name=charter scenario", "-c", "user.email=scenario@example.invalid", ...args],
    {
      cwd,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      env: {
        ...process.env,
        GIT_CONFIG_GLOBAL: "/dev/null",
        GIT_CONFIG_SYSTEM: "/dev/null",
        GIT_AUTHOR_DATE: "2026-01-01T00:00:00+00:00",
        GIT_COMMITTER_DATE: "2026-01-01T00:00:00+00:00",
      },
    },
  );
}

/**
 * The old install, in `home`, which has to be inside the run's tree `tree`: it refuses
 * anything else, so a mistake here can never write the runner's own `$HOME`.
 *
 * `shell` is the profile's program, outside the project, so the project's git status stays
 * clean and the doctor's `rename-plane` has nothing uncommitted to refuse over.
 */
export function anOldInstall(tree: string, home: string, shell: string): OldInstall {
  if (!inside(home, tree)) {
    throw new Error(`the old install's home ${home} is outside the run's tree ${tree}`);
  }
  const configRoot = join(home, ".config");
  const plane = copyFixturePlane();
  const root = realpathSync(plane);
  if (!inside(plane, tree)) {
    throw new Error(`the old install's project ${plane} is outside the run's tree ${tree}`);
  }

  // This machine's harness profile, under the old name of the local settings.
  writeFileSync(
    join(plane, "charter.local.toml"),
    [
      "[harness]",
      'default = "scenario"',
      "",
      "[harness.scenario]",
      'kind = "claude"',
      `command = [${JSON.stringify(shell)}]`,
      "",
    ].join("\n"),
  );

  // A keyring vault, as the old build registered and indexed it, its item in the stub under
  // the old prefix. The fixture's own plain-file vault stays beside it.
  const state = join(plane, ".charter");
  const registry = join(state, "vaults.json");
  const local = JSON.parse(readFileSync(registry, "utf8")) as {
    vaults: Record<string, unknown>;
  };
  local.vaults[OLD_VAULT] = { provider: "keyring", persona: null, config: {} };
  writePrivate(registry, local);
  writePrivate(join(state, "vaults", `${OLD_VAULT}.keys.json`), {
    service: OLD_SERVICE,
    keys: {
      [OLD_SECRET.key]: { size: "16–31 bytes", updated: "2026-09-01T00:00:00Z" },
    },
  });
  writePrivate(join(state, "keyring-stub.json"), {
    [`${OLD_SERVICE}\n${OLD_SECRET.key}`]: OLD_SECRET.value,
  });

  // The project in git, everything committed: `.charter/` and `charter.local.toml` are
  // ignored by the fixture's own `.gitignore`, as a real project's are.
  git(plane, ["init", "-q", "-b", "main", "."]);
  git(plane, ["add", "-A"]);
  git(plane, ["commit", "-q", "-m", "the project as an old charter left it"]);
  const dirty = git(plane, ["status", "--porcelain"]);
  if (dirty !== "") throw new Error(`the old install's project is not clean:\n${dirty}`);

  // The machine store, under the old name: the project remembered, approved and pinned, one
  // workspace pinned in it, and the machine on the dev channel.
  rmSync(join(configRoot, "charter"), { recursive: true, force: true });
  rmSync(join(configRoot, "purlis"), { recursive: true, force: true });
  mkdirSync(configRoot, { recursive: true });
  writePrivate(join(configRoot, "charter", "machine.json"), {
    version: 1,
    at: OLD_APPROVED,
    recents: [
      {
        plane: root,
        opened: OLD_APPROVED,
        trust: { approved: OLD_APPROVED, plugins: {}, env: {}, starts: {}, profiles: {} },
        pinned: true,
        pinnedWorkspaces: [OLD_WORKSPACE_PIN],
        mostActivePinned: true,
      },
    ],
    windows: [],
    channel: OLD_CHANNEL,
  });

  return { home, configRoot, plane, root };
}

/**
 * Takes away what `anOldInstall` made: the project and the config home. The `$HOME` itself is
 * the run's and stays.
 */
export function removeTheOldInstall(old: OldInstall): void {
  rmSync(dirname(old.plane), { recursive: true, force: true });
  rmSync(old.configRoot, { recursive: true, force: true });
}
