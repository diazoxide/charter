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
 * A `charter.local.toml` in `plane` declaring one profile that runs the fake harness.
 *
 * The profile's program is a script rather than `fake-harness` itself, for two reasons that
 * are both the real thing being tested. Charter probes a profile's own command with
 * `plugin list --json` before it will start a chat on it — a chat whose config folder holds
 * no charter plugin looks guarded and is not (ADR 0022) — so the script answers that probe
 * as a wired Claude Code would. And charter puts `--session-id <uuid> --name <name>` on the
 * line for a Claude Code chat, which `fake-harness` has no flags for, so the script drops
 * the arguments the way a wrapper profile does.
 *
 * It is NOT approved here. The approval is the operator's click, and the scenario makes it.
 */
export function declareAProfile(plane: string, fakeHarness: string): void {
  const program = join(plane, "claude-stand-in");
  writeFileSync(
    program,
    [
      "#!/bin/sh",
      "# Written by the scenario tests: a profile's command, which charter probes first.",
      'if [ "$1" = "plugin" ]; then',
      '  echo \'[{"id":"charter@charter","scope":"user","enabled":true}]\'',
      "  exit 0",
      "fi",
      `exec ${JSON.stringify(fakeHarness)} \\`,
      "  --synthetic 4096 \\",
      `  --sentinel ${JSON.stringify(READY)} \\`,
      "  --interactive",
      "",
    ].join("\n"),
  );
  chmodSync(program, 0o755);
  writeFileSync(
    join(plane, "charter.local.toml"),
    [
      "[harness]",
      'default = "scenario"',
      "",
      "[harness.scenario]",
      'kind = "claude"',
      `command = [${JSON.stringify(program)}]`,
      "",
    ].join("\n"),
  );
}
