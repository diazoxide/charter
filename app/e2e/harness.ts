import { chmodSync, mkdirSync, writeFileSync } from "node:fs";
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

/** Where the built binaries are, which CI and a person both pass in. */
export function built(name: string): string {
  const from = process.env.CHARTER_TARGET_DIR ?? join(process.cwd(), "..", "target", "debug");
  return join(from, name);
}
