import { execFileSync } from "node:child_process";
import {
  chmodSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import process from "node:process";

/**
 * What the app runs when a pane asks for a new session.
 *
 * The app opens the operator's shell, so a scenario test gives it one: a script that runs
 * `fake-harness`, which writes the same output every run and then answers what is typed.
 * Nothing about the app knows it is a test.
 */
export const READY = "session ready";

/**
 * The one directory this run makes anything in — every fixture plane, every config home,
 * every stand-in shell — and the fence every charter process the run starts is held to.
 *
 * **charter-app#129 is why it is one directory and not several.** This repository is checked
 * out at `workspaces/ide/charter-app`, inside the operator's own control plane, so an app
 * started here with nothing pinned walks up and finds THAT plane: a scenario run wrote 49 of
 * its own chats into `/Users/aharon/IdeaProjects/charter/.charter/app/reopen.json`, and the
 * operator found them by opening charter. The record is an execution input (ADR 0035), so
 * that is a test suite leaving programs behind in a real plane, not only a mess.
 *
 * Pinning `$CHARTER_ROOT` is how a run *avoids* that, and it is not enough on its own: a
 * config that stops pinning it goes green and poisons a plane. So every process also carries
 * `$CHARTER_PLANE_FENCE` pointing here, and the binary the scenario tests drive is built with
 * the fence in it (`e2e` turns on `charter-core/fenced`). A run that resolves any plane this
 * tree does not hold dies naming it, in the job that broke it.
 *
 * Made once per launcher process, and `wdio.state.conf.ts` and `wdio.bench.conf.ts` spread
 * `wdio.conf.ts`, so all three share it — which is what makes it the fence for all of them.
 *
 * **It has to survive the fork, and that is why it goes through the environment.**
 * WebdriverIO runs each spec file in a worker process, and a worker imports this module
 * again: a plain `mkdtempSync` here would give the worker a SECOND tree, so a plane a spec
 * copies for itself — `opener.e2e.ts`'s stranger, `projects.e2e.ts`'s second project — would
 * land outside the fence the launcher gave the app, and the app would die opening the very
 * plane the spec made for it. The variable is inherited by the fork, so both halves of the
 * run name one tree. A worker that somehow did not inherit it makes its own and the app
 * refuses: wrong, but loudly and in the run that is wrong, which is the whole point.
 */
function theRunsTree(): string {
  const shared = process.env.CHARTER_SCENARIO_RUN;
  if (shared) return shared;
  const made = mkdtempSync(join(tmpdir(), "charter-scenario-run-"));
  process.env.CHARTER_SCENARIO_RUN = made;
  return made;
}

export const THE_RUNS_TREE = theRunsTree();

/**
 * The environment a charter process this run starts is given: the plane it means, a machine
 * store of its own, and the fence.
 *
 * **The three go together, and that is why they are one function.** `CHARTER_ROOT` says
 * which plane; `CHARTER_CONFIG_HOME` keeps this run's approvals and recents out of the
 * runner's own (see `aConfigHomeOfItsOwn`); `CHARTER_PLANE_FENCE` is what turns "the config
 * forgot one of them" from a silent write into a dead app. A launcher that set two of the
 * three used to typecheck, lint and pass — `wdio.bench.conf.ts` set neither of the first two
 * for months.
 */
export function theRunsEnvironment(
  plane: string,
  extra: Record<string, string> = {},
): Record<string, string> {
  return {
    CHARTER_ROOT: plane,
    CHARTER_CONFIG_HOME: aConfigHomeOfItsOwn(),
    CHARTER_PLANE_FENCE: THE_RUNS_TREE,
    ...extra,
  };
}

/**
 * Exactly the `PATH` macOS gives an app opened from Finder, and nothing else.
 *
 * `launchd` starts a GUI process; no login shell is involved, so none of the directories an
 * operator's shell adds are there. charter-app#134 is what that costs.
 */
export const A_FINDER_LAUNCHS_PATH = "/usr/bin:/bin:/usr/sbin:/sbin";

/**
 * A `$HOME` holding a `claude` where Claude Code's own installer puts it — `~/.local/bin` —
 * and nowhere that `A_FINDER_LAUNCHS_PATH` can see.
 *
 * **This is charter-app#134's whole shape** (`wdio.finder.conf.ts`). The operator's harness
 * was installed, findable by their shell, and invisible to the app, so a double-clicked
 * charter refused every chat on a built-in profile. The program is the same wrapper
 * `declareAProfile` writes — it answers `claude plugin list --json` as a wired Claude Code
 * does and then runs the fake harness — but it is reached by its BARE NAME out of the
 * registry, which is the part no other scenario exercises.
 */
export function writeAHarnessOnlyAShellWouldFind(fakeHarness: string): string {
  const home = mkdtempSync(join(THE_RUNS_TREE, "finder-home-"));
  const bin = join(home, ".local", "bin");
  mkdirSync(bin, { recursive: true });
  writeFileSync(join(bin, "claude"), theProfilesProgram(fakeHarness));
  chmodSync(join(bin, "claude"), 0o755);
  return home;
}

export function writeShell(fakeHarness: string): string {
  const where = join(THE_RUNS_TREE, "shells");
  mkdirSync(where, { recursive: true });
  const shell = join(where, "harness-as-a-shell");
  writeFileSync(
    shell,
    [
      "#!/bin/sh",
      "# Written by the scenario tests: the app's idea of a shell, which is the fake harness.",
      `exec ${JSON.stringify(fakeHarness)} \\`,
      "  --synthetic 4096 \\",
      `  --sentinel ${JSON.stringify(READY)} \\`,
      "  --interactive",
      "",
    ].join("\n"),
  );
  chmodSync(shell, 0o755);
  return shell;
}

/**
 * The same, for a harness that reports its state the way a real one does — by running
 * `charter hook` from inside its own session.
 *
 * Nothing is faked past the harness itself: the hook is the real binary, the socket is the
 * one the app opened, and it finds both in the environment the app put the session in. It
 * reports a turn beginning before it writes anything, then holds its output until a line is
 * typed (`--wait-for-input`), so a test can see `running` without racing the turn's end —
 * and fires `stop` once the output is done.
 */
export function writeReportingShell(fakeHarness: string, charter: string): string {
  const where = join(THE_RUNS_TREE, "shells");
  mkdirSync(where, { recursive: true });
  const shell = join(where, "reporting-harness-as-a-shell");
  writeFileSync(
    shell,
    [
      "#!/bin/sh",
      "# Written by the scenario tests: a harness that reports its own state through hooks.",
      "#",
      "# It reports no pid and no conversation id of its own, which is why the profile that",
      "# runs it declares `kind = \"codex\"`. A chat's harness is known from its profile's",
      "# declared kind since M1.2, and Claude Code's rule (ADR 0024) admits a report only if",
      "# it carries a matching `CLAUDE_PID` AND names the conversation charter chose — which",
      "# a shell script cannot do without implementing Claude Code's whole hook payload.",
      "# Codex's rule is the narrow one this stand-in actually meets: the first report of a",
      "# chat is adopted. Declaring it as the kind it BEHAVES like is honest; teaching it to",
      "# impersonate a Claude Code would be a fixture testing itself.",
      "# The turn's rising edge, before a byte of output exists.",
      `${JSON.stringify(charter)} hook userpromptsubmit </dev/null`,
      `exec ${JSON.stringify(fakeHarness)} \\`,
      "  --synthetic 4096 \\",
      "  --wait-for-input \\",
      `  --sentinel ${JSON.stringify(READY)} \\`,
      `  --hook ${JSON.stringify(`${charter} hook stop </dev/null`)} \\`,
      "  --interactive",
      "",
    ].join("\n"),
  );
  chmodSync(shell, 0o755);
  return shell;
}

/** Where the built binaries are, which CI and a person both pass in. */
export function built(name: string): string {
  const from = process.env.CHARTER_TARGET_DIR ?? join(process.cwd(), "..", "target", "debug");
  return join(from, name);
}

/**
 * A writable copy of a fixture plane for the app to be started in.
 *
 * The fixtures are committed and the app writes to a plane, so a scenario test never runs
 * against the ones in the repository. The copy is made fresh for the run and `CHARTER_ROOT`
 * points the app at it, which is how a plane is pinned rather than inherited from whichever
 * directory the test runner happens to be in.
 */
export function copyFixturePlane(name = "daily"): string {
  const from = join(import.meta.dirname, "..", "..", "tests", "fixtures", "planes", name);
  // Inside `THE_RUNS_TREE`, which is the fence: a plane this function made is a plane the
  // run may act on, and there is no other kind (charter-app#129).
  const to = mkdtempSync(join(THE_RUNS_TREE, `plane-${name}-`));
  const root = join(to, name);
  cpSync(from, root, { recursive: true });
  // Directories the fixture cannot carry, because git will not track an empty one. The
  // generator records them beside the plane; restoring them matters because "no todos/" and
  // "an empty todos/" are different starting states.
  const listing = join(
    import.meta.dirname,
    "..",
    "..",
    "tests",
    "fixtures",
    "planes",
    `${name}.empty-dirs`,
  );
  if (existsSync(listing)) {
    for (const rel of readFileSync(listing, "utf8").split(/\s+/).filter(Boolean)) {
      mkdirSync(join(root, rel), { recursive: true });
    }
  }
  return root;
}

/**
 * A `charter.local.toml` in `plane` declaring one profile that runs `program`.
 *
 * The profile's own program is a wrapper around `program`, and both halves of that are the
 * real thing being tested. Charter probes a profile's command with `plugin list --json`
 * before it will start a chat on it — a chat whose config folder holds no charter plugin
 * looks guarded and is not (ADR 0022) — so the wrapper answers that probe as a wired Claude
 * Code would. And charter puts `--session-id <uuid> --name <name>` on the line for a Claude
 * Code chat, which the fake harness has no flags for, so the wrapper drops its arguments
 * exactly as a real wrapper profile does.
 *
 * Two profiles are declared, not one. `scenario` is the default, which every spec that just
 * wants a chat picks; `needs-approval` exists only for the picker scenario's approval test.
 * Approving is recorded per profile and the specs share one app process, so a single profile
 * would make that test depend on running before every other spec — and the glob does not
 * promise that.
 */
export function declareAProfile(plane: string, program: string, kind = "claude"): void {
  // A Codex profile is gated on a file, not on a probe: charter refuses to start a Codex
  // chat unless `$CODEX_HOME/config.toml` carries all three marks — the plugin enabled, the
  // harness named, and a guard hook the operator trusted — because a hook Codex has not
  // trusted is inert, and a plugin nobody approved reads exactly like wired to anything that
  // stops at the plugin table. So a plane that declares one writes that home.
  const env: Record<string, string> = kind === "codex" ? { CODEX_HOME: writeCodexHome(plane) } : {};
  const wrapper = join(plane, "claude-stand-in");
  writeFileSync(wrapper, theProfilesProgram(program));
  chmodSync(wrapper, 0o755);
  writeFileSync(
    join(plane, "charter.local.toml"),
    [
      "[harness]",
      'default = "scenario"',
      "",
      "[harness.scenario]",
      `kind = ${JSON.stringify(kind)}`,
      `command = [${JSON.stringify(wrapper)}]`,
      ...envLines(env),
      "",
      "[harness.needs-approval]",
      `kind = ${JSON.stringify(kind)}`,
      `command = [${JSON.stringify(wrapper)}]`,
      ...envLines(env),
      "",
    ].join("\n"),
  );
}

/**
 * The shell script a profile's command points at: a wired Claude Code's answer to the wiring
 * probe, then `program`.
 *
 * Its own function because two callers write it — `declareAProfile` puts it in the plane as a
 * declared profile's absolute command, and `writeAHarnessOnlyAShellWouldFind` puts the same
 * bytes in a `$HOME/.local/bin/claude` that only a search finds. One copy, or the two drift
 * and the second one stops standing in for the first.
 */
function theProfilesProgram(program: string): string {
  return [
    "#!/bin/sh",
    "# Written by the scenario tests: a profile's command, which charter probes first.",
    'if [ "$1" = "plugin" ]; then',
    '  echo \'[{"id":"charter@charter","scope":"user","enabled":true}]\'',
    "  exit 0",
    "fi",
    "# What the chat's environment says about charter's footer (charter ADR 0029), written",
    "# down where a scenario can read it. `charter statusline` is Claude Code's `statusLine`",
    "# command and inherits this environment; the fake harness runs no such command, so this",
    "# file is how a scenario sees what a real one would have been started with. The chat's",
    "# own number keys it, so two chats in one run never write over each other.",
    `if [ -n "\${CHARTER_ROOT:-}" ] && [ -n "\${CHARTER_CHAT:-}" ]; then`,
    '  mkdir -p "$CHARTER_ROOT/.charter/scenario"',
    "  printf '%s' \"${CHARTER_FOOTER:-}\" \\",
    '    > "$CHARTER_ROOT/.charter/scenario/footer-$CHARTER_CHAT"',
    "fi",
    "# Charter starts a Claude Code chat under an id it chose (`--session-id <uuid>`),",
    "# and a report counts as that chat's harness speaking only if it names the SAME",
    "# id (ADR 0024). A real Claude Code reports the id it was given; this stand-in has",
    "# to as well, so it reads the flag off its own command line and puts it where a",
    "# hook looks. Everything else is dropped, which is what a wrapper profile does.",
    "#",
    "# The session id, the settings charter armed this session with, and the hook socket are",
    "# also written down, per chat, for `gauge.e2e.ts`: a real Claude Code runs the",
    "# `statusLine` those settings name, and the fake harness runs nothing, so the scenario",
    "# runs it itself — the command charter armed, with the environment this chat has.",
    `SEEN=""`,
    `if [ -n "\${CHARTER_ROOT:-}" ] && [ -n "\${CHARTER_CHAT:-}" ]; then`,
    '  SEEN="$CHARTER_ROOT/.charter/scenario-harness"',
    '  mkdir -p "$SEEN"',
    `  printf '%s' "\${CHARTER_HOOK_SOCKET:-}" > "$SEEN/socket-$CHARTER_CHAT"`,
    "fi",
    "while [ $# -gt 0 ]; do",
    '  if [ "$1" = "--session-id" ]; then',
    "    CLAUDE_CODE_SESSION_ID=$2",
    "    export CLAUDE_CODE_SESSION_ID",
    `    [ -n "$SEEN" ] && printf '%s' "$2" > "$SEEN/session-$CHARTER_CHAT"`,
    "  fi",
    '  if [ "$1" = "--settings" ]; then',
    `    [ -n "$SEEN" ] && printf '%s' "$2" > "$SEEN/settings-$CHARTER_CHAT"`,
    "  fi",
    "  shift",
    "done",
    `exec ${JSON.stringify(program)}`,
    "",
  ].join("\n");
}

/** A profile's `env` table, or nothing when it sets none. */
function envLines(env: Record<string, string>): string[] {
  const names = Object.keys(env);
  if (names.length === 0) return [];
  return [`env = { ${names.map((n) => `${n} = ${JSON.stringify(env[n])}`).join(", ")} }`];
}

/**
 * A `CODEX_HOME` inside `plane` carrying the three marks charter requires, and an installed
 * plugin whose `hooks.json` places charter's guard.
 *
 * The trust key is spelled the way Codex spells it — `<plugin>:hooks/hooks.json:<event>:
 * <group>:<hook>`, with the event in snake case — and the position is read out of the
 * INSTALLED plugin's own manifest rather than hard-coded, which is why this writes a
 * manifest at all rather than only a config.
 */
function writeCodexHome(plane: string): string {
  const home = join(plane, "codex-home");
  const hooks = join(home, "plugins", "cache", "charter", "charter", "0.62.1", "hooks");
  mkdirSync(hooks, { recursive: true });
  writeFileSync(
    join(hooks, "hooks.json"),
    JSON.stringify({
      hooks: { PreToolUse: [{ hooks: [{ command: "charter hook pretooluse" }] }] },
    }),
  );
  writeFileSync(
    join(home, "config.toml"),
    [
      '[plugins."charter@charter"]',
      "enabled = true",
      "",
      "[shell_environment_policy.set]",
      'CHARTER_HARNESS = "codex"',
      "",
      '[hooks.state."charter@charter:hooks/hooks.json:pre_tool_use:0:0"]',
      'trusted_hash = "written-by-the-scenario-tests"',
      "",
    ].join("\n"),
  );
  return home;
}

/**
 * Make the fixture plane's repo directories into real clones.
 *
 * The committed fixture cannot carry them: git will not track a `.git` directory inside a
 * repository, so `tests/fixtures/planes/daily` holds `svc` and `tool` as ordinary
 * directories. The panels are about what git says, so the copy the run works on gets the
 * real thing — built here, from the files the fixture already has.
 *
 * `tool` is left with something uncommitted, because "clean" and "dirty" are two different
 * rows and a fixture where every repo is clean cannot tell them apart.
 */
export function cloneTheFixtureRepos(plane: string, workspace = "alpha"): void {
  for (const name of ["svc", "tool"]) {
    const repo = join(plane, "workspaces", workspace, name);
    if (!existsSync(repo)) continue;
    git(repo, ["init", "-q", "-b", "main", "."]);
    git(repo, ["add", "-A"]);
    git(repo, ["commit", "-q", "-m", "the fixture as it was committed"]);
  }
  writeFileSync(join(plane, "workspaces", workspace, "tool", "scratch.txt"), "not committed\n");
}

/**
 * Cut one real piece off the fixture's `svc` clone, where charter keeps them.
 *
 * `workspaces/<ws>/.worktrees/<repo>/<piece>` is the only place `worktree::list` looks — it
 * filters git's own listing down to registrations under that root, so a tree cut anywhere
 * else is not this workspace's and is not shown. Cut here with plain git and NOT by charter,
 * which is what makes it read `unwired`: the explorer has to say so before a chat is started
 * in a tree that would run with none of the plane's ask/deny rules.
 */
export function cutAFixturePiece(plane: string, workspace = "alpha"): void {
  const repo = join(plane, "workspaces", workspace, "svc");
  if (!existsSync(repo)) return;
  const at = join(plane, "workspaces", workspace, ".worktrees", "svc", "fix-login");
  mkdirSync(join(plane, "workspaces", workspace, ".worktrees", "svc"), { recursive: true });
  git(repo, ["worktree", "add", "-q", "-b", "fix-login", at]);
}

/**
 * The forge cache a refresher would have left behind, keyed the way it keys it: by the
 * checkout's path, written out.
 *
 * Only `svc` gets an entry. `tool` having none is half the point — the panel has to say that
 * nobody has fetched it rather than leaving the cell blank, which reads as green.
 */
export function writeForgeCache(plane: string, workspace = "alpha"): void {
  const cache = join(plane, ".charter", "cache");
  mkdirSync(cache, { recursive: true });
  writeFileSync(
    join(cache, "glstate.json"),
    JSON.stringify({
      [join(plane, "workspaces", workspace, "svc")]: {
        branch: "main",
        ts: Math.floor(Date.now() / 1000) - 120,
        ci: "failed",
        change: 41,
        sigil: "#",
      },
    }),
  );
}

/**
 * git, for the test's own setup. Never the code under test, and never the operator's own
 * configuration: a developer's `init.defaultBranch`, hooks or commit template must not reach
 * a fixture, and a CI runner has no identity configured at all.
 */
function git(cwd: string, args: string[]): void {
  execFileSync(
    "git",
    ["-c", "user.name=charter scenario", "-c", "user.email=scenario@example.invalid", ...args],
    {
      cwd,
      stdio: "pipe",
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
 * Leaves `plane` with no record of what was open, which is what a first launch reads.
 *
 * The app writes `.charter/app/reopen.json` into the plane it was launched in. It always
 * meant to; until the launch and the commands agreed on one resolver it silently did not,
 * because the launch resolved the working directory while every command resolved
 * `$CHARTER_ROOT` (charter-app#109). Now that it does, one plane copy shared by more than one
 * session would hand the second session the first one's chats — a spec testing the record
 * instead of itself. Every session starts from none.
 */
export function anEmptyRecord(plane: string): void {
  rmSync(join(plane, ".charter", "app", "reopen.json"), { force: true });
}

/**
 * A config home of this run's own, so charter's machine store is empty when the app starts.
 *
 * The store holds which projects this machine remembers and which the operator has approved
 * (charter ADR 0034), and it lives under `$CHARTER_CONFIG_HOME`, else `$XDG_CONFIG_HOME`,
 * else `~/.config`. Left alone, a scenario run would read and WRITE the runner's own — so
 * "charter asks about a project nobody has approved" would pass on a fresh runner and fail on
 * the second run of the same one, which is the worst kind of green.
 *
 * `$CHARTER_CONFIG_HOME` and not `$XDG_CONFIG_HOME`: `gh` keeps its auth under the second, so
 * redirecting that one to isolate charter silently logs `gh` out. That is the reason the
 * variable exists, and it is `report.py:consent_path`'s reason, unchanged.
 */
export function aConfigHomeOfItsOwn(): string {
  return mkdtempSync(join(THE_RUNS_TREE, "config-"));
}
