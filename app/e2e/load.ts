import { chmodSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

/**
 * What each session the benchmark opens runs.
 *
 * The app runs the operator's shell in a new pane, so the benchmark gives it one that runs
 * `fake-harness` with whatever load `nextLoad` last wrote. Choose the load, then press
 * "New tab" or split: that session runs it. Nothing about the app knows it is measured.
 */
const where = join(tmpdir(), "charter-bench");
const loadFile = join(where, "load");

/** Quoted for `sh`, so a path with a space in it is still one argument. */
function quoted(arg: string): string {
  return `'${arg.split("'").join(`'\\''`)}'`;
}

export function writeBenchShell(fakeHarness: string): string {
  mkdirSync(where, { recursive: true });
  const shell = join(where, "harness-as-a-shell");
  writeFileSync(
    shell,
    [
      "#!/bin/sh",
      "# Written by the benchmark: the fake harness, with the load the benchmark chose last.",
      `eval "exec ${quoted(fakeHarness).split('"').join('\\"')} $(cat ${quoted(loadFile)})"`,
      "",
    ].join("\n"),
  );
  chmodSync(shell, 0o755);
  nextLoad(["--interactive"]);
  return shell;
}

/** The arguments the next session's fake harness runs with. */
export function nextLoad(args: string[]): void {
  mkdirSync(where, { recursive: true });
  writeFileSync(loadFile, args.map(quoted).join(" "));
}

/** A file of `lines` plain lines that scroll, for filling a session's history. */
export function scrollingLines(lines: number): { path: string; bytes: number } {
  mkdirSync(where, { recursive: true });
  const path = join(where, `lines-${lines}.raw`);
  let text = "";
  for (let n = 1; n <= lines; n++) {
    text += `\x1b[38;5;${16 + (n % 216)}m${String(n).padStart(6, "0")}\x1b[0m ${"history ".repeat(12)}\r\n`;
  }
  writeFileSync(path, text);
  return { path, bytes: Buffer.byteLength(text) };
}

/**
 * One synchronized repaint — coloured lines inside `?2026h` … `?2026l` — as a file, so a load
 * can write each repaint whole, in one write, the way a harness drawing a frame does.
 *
 * `columns` and `runs` decide how big that write is: a harness drawing a full screen with
 * colour through it writes tens of kilobytes at once, which a pseudo-terminal may hand over
 * in pieces however the harness wrote it.
 */
export function synchronizedFrame(
  name: string,
  { rows = 40, columns = 60, runs = 1 } = {},
): { path: string; bytes: number } {
  mkdirSync(where, { recursive: true });
  const path = join(where, `synchronized-frame-${name}.raw`);
  let text = "\x1b[?2026h\x1b[H\x1b[2J";
  for (let row = 0; row < rows; row++) {
    const width = Math.max(1, Math.floor(columns / runs));
    for (let run = 0; run < runs; run++) {
      text += `\x1b[38;5;${16 + ((row * runs + run) % 216)}m${"session ".repeat(width).slice(0, width)}\x1b[0m`;
    }
    text += "\r\n";
  }
  text += "\x1b[?2026l";
  writeFileSync(path, text);
  return { path, bytes: Buffer.byteLength(text) };
}
