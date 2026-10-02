/// <reference types="node" />
/**
 * Test-only: what `git diff` itself answers for two texts, as hunks. The oracle the light
 * editor's merge view is checked against (ADR 0084 §2: *"Which lines changed is git's
 * answer"*), until RC-2's engine hands the window the same thing.
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { GitHunk } from "./hunks";

export function gitHunks(base: string, head: string): GitHunk[] {
  const dir = mkdtempSync(join(tmpdir(), "charter-rc5-"));
  try {
    writeFileSync(join(dir, "a"), base);
    writeFileSync(join(dir, "b"), head);
    let out = "";
    try {
      out = execFileSync("git", ["diff", "--no-index", "--no-color", "-U0", "a", "b"], {
        cwd: dir,
        encoding: "utf8",
      });
    } catch (ran) {
      // `--no-index` exits 1 when the files differ, which is the case worth testing.
      out = (ran as { stdout: string }).stdout;
    }
    return [...out.matchAll(/^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/gm)].map((hit) => ({
      oldStart: Number(hit[1]),
      oldLines: hit[2] === undefined ? 1 : Number(hit[2]),
      newStart: Number(hit[3]),
      newLines: hit[4] === undefined ? 1 : Number(hit[4]),
    }));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
