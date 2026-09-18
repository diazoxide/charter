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
      "# It names its own process the way Claude Code does. Since M1.2 a chat's harness comes",
      '# from its profile\'s declared KIND, so a chat on a `kind = "claude"` profile is judged',
      "# by Claude Code's rule: a report is its harness speaking only if it carries a matching",
      "# `CLAUDE_PID` (ADR 0024). A stand-in that never set one was adopted under the narrower",
      "# pid-less rule while the app thought it was running a shell, and stopped being adopted",
      "# the moment the app knew better.",
      "CLAUDE_PID=$$",
      "export CLAUDE_PID",
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
export function declareAProfile(plane: string, program: string): void {
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
      'kind = "claude"',
      `command = [${JSON.stringify(wrapper)}]`,
      "",
      "[harness.needs-approval]",
      'kind = "claude"',
      `command = [${JSON.stringify(wrapper)}]`,
      "",
    ].join("\n"),
  );
}
