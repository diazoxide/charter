/**
 * What the app costs in memory beyond its own process: the web content process that draws its
 * window, and the harnesses its chats run (SC-1, #679).
 *
 * `processes.ts` asks the operating system; this file only reads what it answered, so the
 * reading can be tested without an app, a WebKit or a harness on the machine.
 */

/** One line of `ps -A -o pid=,ppid=,rss=,etime=,command=`. */
export interface ProcessRow {
  pid: number;
  ppid: number;
  /** Resident memory, in kilobytes. */
  rssKb: number;
  /** How long the process has been running, in whole seconds. */
  seconds: number;
  command: string;
}

/** The columns `parseProcesses` reads, in the order it reads them. */
export const PS_COLUMNS = "pid=,ppid=,rss=,etime=,command=";

/** `[[dd-]hh:]mm:ss`, the elapsed time both macOS's and Linux's `ps` print, in seconds. */
function elapsed(text: string): number {
  const [days, clock] = text.includes("-") ? text.split("-") : ["0", text];
  const parts = clock.split(":").map((part) => Number.parseInt(part, 10));
  while (parts.length < 3) parts.unshift(0);
  const [hours, minutes, seconds] = parts;
  return Number.parseInt(days, 10) * 86_400 + hours * 3_600 + minutes * 60 + seconds;
}

/** Every process `ps` listed, skipping any line it could not read. */
export function parseProcesses(text: string): ProcessRow[] {
  const rows: ProcessRow[] = [];
  for (const line of text.split("\n")) {
    const found = /^\s*(\d+)\s+(\d+)\s+(\d+)\s+(\S+)\s+(.*)$/.exec(line);
    if (found === null) continue;
    rows.push({
      pid: Number.parseInt(found[1], 10),
      ppid: Number.parseInt(found[2], 10),
      rssKb: Number.parseInt(found[3], 10),
      seconds: elapsed(found[4]),
      command: found[5],
    });
  }
  return rows;
}

/** A set of processes and the resident memory they hold together. */
export interface Footprint {
  processes: number[];
  rssKb: number;
}

function total(rows: ProcessRow[]): Footprint {
  return {
    processes: rows.map((row) => row.pid),
    rssKb: rows.reduce((sum, row) => sum + row.rssKb, 0),
  };
}

/** The last path segment of the program a command line runs. */
function program(command: string): string {
  const first = command.split(" ")[0];
  return first.slice(first.lastIndexOf("/") + 1);
}

/**
 * The web content processes that draw the window of the app at `appPid`: ADR 0086's M2, and
 * with the app's own memory M1.
 *
 * **macOS gives no parent to follow.** WebKit's WebContent is an XPC service, so launchd
 * starts it and is its parent. What ties one to this app is time: it was started after the
 * app was. On a CI runner nothing else draws web pages, and a page some other program opened
 * before the app started is left out by the same rule. The count is kept beside the sum, so a
 * stray one shows as a second process rather than as hidden memory.
 *
 * **Linux follows the parent.** WebKitGTK starts `WebKitWebProcess` as a child of the app.
 * The network process is not web content on either system.
 */
export function webContent(rows: ProcessRow[], appPid: number, platform: string): Footprint {
  const app = rows.find((row) => row.pid === appPid);
  if (app === undefined) return total([]);
  if (platform === "darwin") {
    return total(
      rows.filter(
        (row) =>
          program(row.command) === "com.apple.WebKit.WebContent" && row.seconds <= app.seconds,
      ),
    );
  }
  return total(
    rows.filter((row) => row.ppid === appPid && program(row.command) === "WebKitWebProcess"),
  );
}

/** The harnesses' memory: their sum, and the largest one with everything it started. */
export interface Harnesses extends Footprint {
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

/**
 * Each harness the app at `appPid` runs, counted with every process it started: its tool
 * commands and their children, which are the harness's child runs. ADR 0086's F1 is this per
 * harness, measured on real harnesses by the release scale run (#814). In CI it is the fake
 * harness, which says what charter's own tree adds, not what a real harness costs.
 *
 * A harness is a process anywhere under the app whose program is `harness`, so it is found
 * whether the app starts it or, once ADR 0068 lands, the host does. A harness under another
 * harness is part of the outer one.
 */
export function harnesses(rows: ProcessRow[], appPid: number, harness: string): Harnesses {
  const app = rows.find((row) => row.pid === appPid);
  if (app === undefined) return { processes: [], rssKb: 0, largestKb: 0 };
  const roots: ProcessRow[] = [];
  const walk = (parent: ProcessRow) => {
    for (const row of rows.filter((child) => child.ppid === parent.pid)) {
      if (program(row.command) === harness) roots.push(row);
      else walk(row);
    }
  };
  walk(app);
  const sizes = roots.map((root) => total(tree(rows, root)).rssKb);
  return {
    processes: roots.map((root) => root.pid),
    rssKb: sizes.reduce((sum, size) => sum + size, 0),
    largestKb: sizes.reduce((most, size) => Math.max(most, size), 0),
  };
}

const UNIT_KB: Record<string, number> = { B: 1 / 1024, KB: 1, MB: 1024, GB: 1024 * 1024 };

/**
 * The total `footprint -p <pid>` prints for a process, in kilobytes, or `null` when it printed
 * none.
 *
 * **Why footprint and not only RSS, on macOS.** ADR 0082 measured the web content process with
 * `footprint`: 813 MB, peak 1,529 MB. Resident memory leaves out what macOS has compressed or
 * swapped, and a web content process under pressure is most of that, so its RSS reads low.
 * Both are recorded, so a row can be read against either.
 */
export function footprintKb(text: string): number | null {
  const found = /Footprint:\s*([\d.]+)\s*(B|KB|MB|GB)\b/.exec(text);
  if (found === null) return null;
  return Number.parseFloat(found[1]) * UNIT_KB[found[2]];
}
