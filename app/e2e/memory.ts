/**
 * What the app costs in memory beyond its own process: the web content process that draws its
 * window, and the harnesses its chats run (SC-1, #679).
 *
 * `processes.ts` asks the operating system; this file only reads what it answered, so the
 * reading can be tested without an app, a WebKit or a harness on the machine.
 */

/** The program the scenario tests run in every pane. */
export const HARNESS = "fake-harness";

/** One process, from `ps`. */
export interface ProcessRow {
  pid: number;
  ppid: number;
  /** Resident memory, in kilobytes. */
  rssKb: number;
  /** How long the process has been running, in whole seconds. */
  seconds: number;
  /** The command line, arguments included. */
  command: string;
  /** The program it runs, by name: the last segment of its path, which may hold spaces. */
  program: string;
}

/** The columns `parseProcesses` reads from its first text, in the order it reads them. */
export const PS_COLUMNS = "pid=,ppid=,rss=,etime=,command=";

/**
 * The columns it reads from its second: the program alone. A command line cannot say where
 * the program's path ends and its arguments begin when the path holds a space; `comm` is the
 * program without its arguments. macOS prints its whole path, Linux its name cut to fifteen
 * characters, and `runs` reads either.
 */
export const PROGRAM_COLUMNS = "pid=,comm=";

/** `[[dd-]hh:]mm:ss`, the elapsed time both macOS's and Linux's `ps` print, in seconds. */
function elapsed(text: string): number {
  const [days, clock] = text.includes("-") ? text.split("-") : ["0", text];
  const parts = clock.split(":").map((part) => Number.parseInt(part, 10));
  while (parts.length < 3) parts.unshift(0);
  const [hours, minutes, seconds] = parts;
  return Number.parseInt(days, 10) * 86_400 + hours * 3_600 + minutes * 60 + seconds;
}

/** Every process `ps` listed in `table` (`PS_COLUMNS`), named from `programs` (`PROGRAM_COLUMNS`). */
export function parseProcesses(table: string, programs: string): ProcessRow[] {
  const names = new Map<number, string>();
  for (const line of programs.split("\n")) {
    const found = /^\s*(\d+)\s+(.+)$/.exec(line);
    if (found === null) continue;
    const path = found[2].trimEnd();
    names.set(Number.parseInt(found[1], 10), path.slice(path.lastIndexOf("/") + 1));
  }
  const rows: ProcessRow[] = [];
  for (const line of table.split("\n")) {
    const found = /^\s*(\d+)\s+(\d+)\s+(\d+)\s+(\S+)\s+(.*)$/.exec(line);
    if (found === null) continue;
    const pid = Number.parseInt(found[1], 10);
    rows.push({
      pid,
      ppid: Number.parseInt(found[2], 10),
      rssKb: Number.parseInt(found[3], 10),
      seconds: elapsed(found[4]),
      command: found[5],
      program: names.get(pid) ?? "",
    });
  }
  return rows;
}

/** Linux keeps a program's name in fifteen characters. */
const LINUX_NAME = 15;

/**
 * Whether a process whose program is `program` (a `ProcessRow`'s) runs `name`: the same name,
 * or, on Linux, its first fifteen characters.
 */
export function runs(program: string, name: string): boolean {
  return (
    program === name || (program.length === LINUX_NAME && name.slice(0, LINUX_NAME) === program)
  );
}

/** A set of processes and the resident memory they hold together. */
export interface Resident {
  pids: number[];
  rssKb: number;
}

function resident(rows: ProcessRow[]): Resident {
  return {
    pids: rows.map((row) => row.pid),
    rssKb: rows.reduce((sum, row) => sum + row.rssKb, 0),
  };
}

/**
 * The web content processes that draw the window of the app at `appPid`: ADR 0086's M2, and
 * with the app's own memory M1.
 *
 * **macOS gives no parent to follow, and this rule holds on CI only.** WebKit's WebContent is
 * an XPC service, so launchd starts it and is its parent. What ties one to this app here is
 * time: it was started after the app was. On a CI runner nothing else draws web pages. On a
 * developer's own machine, a page Safari or Mail opens after the app started is counted too,
 * so a local stress run's web content numbers are an upper bound. The count is kept beside the
 * sum, so a stray process shows as one more process rather than as hidden memory.
 *
 * Asking the app for its own WebContent pid was looked at and not done: neither Tauri nor wry
 * exposes it, and WebKit gives it only through a private `WKWebView` property, which the app
 * would have to read through Objective-C and hand to the test over a channel of its own.
 *
 * **Linux follows the parent.** WebKitGTK starts `WebKitWebProcess` as a child of the app.
 * The network process is not web content on either system.
 */
export function webContent(rows: ProcessRow[], appPid: number, platform: string): Resident {
  const app = rows.find((row) => row.pid === appPid);
  if (app === undefined) return resident([]);
  if (platform === "darwin") {
    return resident(
      rows.filter(
        (row) => runs(row.program, "com.apple.WebKit.WebContent") && row.seconds <= app.seconds,
      ),
    );
  }
  return resident(
    rows.filter((row) => row.ppid === appPid && runs(row.program, "WebKitWebProcess")),
  );
}

/** The harnesses' memory: their sum, and the largest one with everything it started. */
export interface HarnessMemory extends Resident {
  largestKb: number;
}

/** `root` and every process under it, by parent. */
function tree(rows: ProcessRow[], root: ProcessRow): ProcessRow[] {
  const found = [root];
  for (let i = 0; i < found.length; i++) {
    for (const row of rows) if (row.ppid === found[i].pid) found.push(row);
  }
  return found;
}

/** Whether `row` is the session host, `charterd`, which is `charter serve` (ADR 0068 §1). */
function isTheHost(row: ProcessRow): boolean {
  return runs(row.program, "charter") && /\sserve(\s|$)/.test(row.command);
}

/**
 * Each harness the app at `appPid` runs, counted with every process it started: its tool
 * commands and their children, which are the harness's child runs. ADR 0086's F1 is this per
 * harness, measured on real harnesses by the release scale run (#814). In CI it is the fake
 * harness, which says what charter's own tree adds, not what a real harness costs.
 *
 * A harness is a process whose program is `HARNESS`, anywhere under the app **or under the
 * session host**. Under ADR 0068 a chat lives in `charterd`, which launchd or systemd starts,
 * not the app, so once chats move there a search under the app alone would find none. A
 * harness under another harness is part of the outer one.
 */
export function harnessMemory(rows: ProcessRow[], appPid: number): HarnessMemory {
  const roots: ProcessRow[] = [];
  const walk = (parent: ProcessRow) => {
    for (const row of rows.filter((child) => child.ppid === parent.pid)) {
      if (runs(row.program, HARNESS)) roots.push(row);
      else walk(row);
    }
  };
  for (const host of rows.filter((row) => row.pid === appPid || isTheHost(row))) walk(host);
  const sizes = roots.map((root) => resident(tree(rows, root)).rssKb);
  return {
    pids: roots.map((root) => root.pid),
    rssKb: sizes.reduce((sum, size) => sum + size, 0),
    largestKb: sizes.reduce((most, size) => Math.max(most, size), 0),
  };
}

const UNIT_KB: Record<string, number> = { B: 1 / 1024, KB: 1, MB: 1024, GB: 1024 * 1024 };

function kilobytes(amount: string, unit: string): number {
  return Number.parseFloat(amount) * UNIT_KB[unit];
}

/** One process's memory as macOS's `footprint` counts it. */
export interface FootprintOf {
  /** `phys_footprint`: what the process holds now, compressed and swapped memory included. */
  footprintKb: number;
  /** `phys_footprint_peak`: the most it has held. ADR 0082's 1,529 MB is this number. */
  peakKb: number;
}

/** What one `footprint` call for several pids said. */
export interface Footprints {
  byPid: Record<number, FootprintOf>;
  /** Why some pid has no reading, or `null` when every pid asked for has one. */
  error: string | null;
}

/**
 * Reads `footprint -p <pid> -p <pid> …`: each process's section starts with `name [pid]: …
 * Footprint:` and carries its `phys_footprint` and `phys_footprint_peak` lines. A pid asked
 * for that has no section is an error, in footprint's own words where it printed some.
 *
 * **Why footprint and not only RSS, on macOS.** Resident memory leaves out what macOS has
 * compressed or swapped, and a web content process under pressure is most of that: a live one
 * measured 148 MB resident, 2.6 GB by footprint, and a 4 GB peak.
 */
export function parseFootprints(text: string, asked: number[]): Footprints {
  const byPid: Record<number, FootprintOf> = {};
  const complaints: string[] = [];
  let current: number | null = null;
  let now: number | null = null;
  for (const line of text.split("\n")) {
    const header = /^\S.* \[(\d+)\]: .*Footprint:/.exec(line);
    if (header !== null) {
      current = Number.parseInt(header[1], 10);
      now = null;
      continue;
    }
    if (line.startsWith("footprint: ")) {
      complaints.push(line.trim());
      continue;
    }
    const peak = /^\s+phys_footprint_peak:\s*([\d.]+)\s*(B|KB|MB|GB)\b/.exec(line);
    if (peak !== null && current !== null && now !== null) {
      byPid[current] = { footprintKb: now, peakKb: kilobytes(peak[1], peak[2]) };
      current = null;
      continue;
    }
    const held = /^\s+phys_footprint:\s*([\d.]+)\s*(B|KB|MB|GB)\b/.exec(line);
    if (held !== null && current !== null) now = kilobytes(held[1], held[2]);
  }
  const missing = asked.filter((pid) => !(pid in byPid));
  if (missing.length === 0) return { byPid, error: null };
  const said = complaints.filter((complaint) =>
    missing.some((pid) => complaint.includes(`'${pid}'`)),
  );
  return {
    byPid,
    error:
      said.length > 0 ? said.join("; ") : `footprint printed nothing for pid ${missing.join(", ")}`,
  };
}
