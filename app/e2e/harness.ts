import {
  chmodSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
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

export function writeShell(fakeHarness: string): string {
  const where = join(tmpdir(), "charter-scenario");
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
  const where = join(tmpdir(), "charter-scenario");
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
  const to = mkdtempSync(join(tmpdir(), `charter-scenario-plane-${name}-`));
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
  writeFileSync(
    wrapper,
    [
      "#!/bin/sh",
      "# Written by the scenario tests: a profile's command, which charter probes first.",
      'if [ "$1" = "plugin" ]; then',
      '  echo \'[{"id":"charter@charter","scope":"user","enabled":true}]\'',
      "  exit 0",
      "fi",
      "# Charter starts a Claude Code chat under an id it chose (`--session-id <uuid>`),",
      "# and a report counts as that chat's harness speaking only if it names the SAME",
      "# id (ADR 0024). A real Claude Code reports the id it was given; this stand-in has",
      "# to as well, so it reads the flag off its own command line and puts it where a",
      "# hook looks. Everything else is dropped, which is what a wrapper profile does.",
      "while [ $# -gt 0 ]; do",
      '  if [ "$1" = "--session-id" ]; then',
      "    CLAUDE_CODE_SESSION_ID=$2",
      "    export CLAUDE_CODE_SESSION_ID",
      "  fi",
      "  shift",
      "done",
      `exec ${JSON.stringify(program)}`,
      "",
    ].join("\n"),
  );
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
