import { execFileSync, spawnSync } from "node:child_process";
import {
  appendFileSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import process from "node:process";
import {
  HARNESS,
  PROGRAM_COLUMNS,
  PS_COLUMNS,
  type ProcessRow,
  harnessMemory,
  parseFootprints,
  parseProcesses,
  runs,
  webContent,
} from "./memory.js";

/**
 * The app under test and the harnesses it runs, as the operating system sees them — and what
 * is left to look at when the app is gone.
 *
 * The app died once while a scenario was opening its fiftieth tab (charter-app#16), and all
 * that run kept was a WebDriver error: the window's server had stopped answering. Whether the
 * process had died or only that server, how, and at what size, nobody could say. Everything
 * here answers one of those questions, and a failed run writes the answers into `logs/`,
 * which CI keeps.
 */

/** Where a failed run's evidence goes: the directory CI uploads when the scenario job fails. */
export const LOGS = join(process.cwd(), "logs");

/**
 * Where the app under test writes a panic on its way out (`CHARTER_PANIC_LOG`): beside the
 * rest of the evidence, so a run that lost the app keeps the one line that says why.
 */
export const PANIC_LOG = join(LOGS, "panics.log");

/** When this run started, so evidence is only what this run left behind. */
const RUN_STARTED = Date.now();

function ps(args: string[]): string {
  try {
    return execFileSync("ps", args, { encoding: "utf8" });
  } catch {
    // `ps -p` exits non-zero when no such process exists, which is an answer, not a failure.
    return "";
  }
}

/** Every process on this machine, with the program each one runs. */
function processes(): ProcessRow[] {
  return parseProcesses(ps(["-A", "-o", PS_COLUMNS]), ps(["-A", "-o", PROGRAM_COLUMNS]));
}

/**
 * How many fake harnesses this machine is running, asked of the operating system.
 *
 * By the program each process RUNS, not any command line that mentions the name: a Claude
 * Code session whose prompt discussed `fake-harness` was once counted three times, and this
 * test failed at 53 of an expected 50 with nothing wrong. `runs` is the one matcher, shared
 * with `harnessMemory`.
 */
export function harnessesRunning(): number {
  return processes().filter((row) => runs(row.program, HARNESS)).length;
}

/** The processes running `binary`, by pid. The app under test is one of them. */
export function running(binary: string): number[] {
  return ps(["-A", "-o", "pid=,command="])
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => {
      const command = line.slice(line.indexOf(" ") + 1).trimStart();
      return command === binary || command.startsWith(`${binary} `);
    })
    .map((line) => Number.parseInt(line, 10))
    .filter((pid) => Number.isInteger(pid));
}

/** One look at a process: how much memory it holds, how many threads and files it has open. */
export interface Sample {
  pid: number;
  /** Resident memory, in kilobytes. */
  rssKb: number;
  threads: number;
  /** Open descriptors: `/proc` on Linux, `lsof` on macOS (SC-15); `null` where neither answers. */
  descriptors: number | null;
}

/** A look at `pid`, or `null` when it is not running. */
export function sample(pid: number): Sample | null {
  const rss = Number.parseInt(ps(["-o", "rss=", "-p", String(pid)]).trim(), 10);
  if (!Number.isFinite(rss)) {
    return null;
  }
  let threads: number;
  let descriptors: number | null = null;
  if (process.platform === "linux") {
    threads = Number.parseInt(ps(["-o", "nlwp=", "-p", String(pid)]).trim(), 10);
    try {
      descriptors = readdirSync(`/proc/${pid}/fd`).length;
    } catch {
      descriptors = null;
    }
  } else {
    // macOS has no thread-count column; `-M` prints one line per thread under a header.
    threads =
      ps(["-M", "-p", String(pid)])
        .split("\n")
        .filter((line) => line.trim() !== "").length - 1;
    if (process.platform === "darwin") descriptors = descriptorsOnMacos(pid);
  }
  return { pid, rssKb: rss, threads, descriptors };
}

/**
 * How many descriptors `lsof -F f` listed: its `f` lines that are numbers. The others name what
 * is not a descriptor: the working directory (`cwd`), the program's text (`txt`) and each
 * library mapped into it.
 */
export function descriptorsListed(listed: string): number {
  return listed.split("\n").filter((line) => /^f\d+$/.test(line)).length;
}

/**
 * The app's open descriptors on macOS, which has no `/proc`: `lsof`, the one tool every Mac
 * has that lists another process's descriptors. About a tenth of a second at fifty chats, and
 * a stress look is every ten. `-n -P` keep it from resolving names, `-a -d 0-999999` from
 * listing anything but numbered descriptors in the first place. `null` when it does not
 * answer, as the Linux count is when `/proc` does not.
 */
function descriptorsOnMacos(pid: number): number | null {
  try {
    const listed = execFileSync(
      "lsof",
      ["-n", "-P", "-a", "-p", String(pid), "-d", "0-999999", "-F", "f"],
      { encoding: "utf8", timeout: 20_000 },
    );
    return descriptorsListed(listed);
  } catch (err) {
    // `lsof` exits 1 when it warns about one entry it could not read; what it listed is
    // still the answer.
    const stdout = (err as { stdout?: string }).stdout;
    return stdout ? descriptorsListed(stdout) : null;
  }
}

/**
 * The web content process's memory and the harnesses', for `stress.jsonl` (SC-1, #679). Each
 * field feeds a row of ADR 0086's performance budgets:
 *
 * - `webContentKb`, `webContentProcesses`: the web content processes' resident memory, and how
 *   many there are. **M2 on Linux**, and with the app's own `rssKb` M1 there.
 * - `webContentFootprintKb`, `webContentFootprintPeakKb`: the same processes by `footprint`, on
 *   macOS only: their `phys_footprint` summed, and the `phys_footprint_peak` of the largest.
 *   **M2 on macOS is the peak**, the number ADR 0082's 1,529 MB was. Resident memory reads far
 *   low there. So each system is its own baseline: M2 on macOS and M2 on Linux are not one
 *   series. `null` on Linux.
 * - `footprintError`: why `footprint` gave no reading for some process, in its own words, or
 *   `null`. A missing reading is never a silent `null`.
 * - `harnessKb`, `harnessLargestKb`: the harnesses' memory with their child runs, together and
 *   the largest one. F1's measure. In CI the harness is the fake one, so these say what the
 *   harness side of charter's tree costs, and the real harnesses' F1 values come from the
 *   release scale run (#814), which takes them the same way.
 *
 * Two `ps` calls and, on macOS, one `footprint` call for every web content process together.
 */
export interface WebAndHarnessMemory {
  webContentKb: number;
  webContentProcesses: number;
  webContentFootprintKb: number | null;
  webContentFootprintPeakKb: number | null;
  footprintError: string | null;
  harnessKb: number;
  harnessLargestKb: number;
}

/** One `footprint` call for every pid in `pids`, as text: what it printed, both streams. */
function footprint(pids: number[]): string {
  const answered = spawnSync(
    "footprint",
    pids.flatMap((pid) => ["-p", String(pid)]),
    {
      encoding: "utf8",
      timeout: 20_000,
    },
  );
  if (answered.error !== undefined) return `footprint: ${answered.error.message}`;
  return `${answered.stderr ?? ""}\n${answered.stdout ?? ""}`;
}

/** The web content process's and the harnesses' memory, for the app at `pid`. */
export function webAndHarnessMemory(pid: number): WebAndHarnessMemory {
  const rows = processes();
  const web = webContent(rows, pid, process.platform);
  let webContentFootprintKb: number | null = null;
  let webContentFootprintPeakKb: number | null = null;
  let footprintError: string | null = null;
  if (process.platform === "darwin") {
    if (web.pids.length === 0) {
      footprintError = "no web content process was found to ask footprint about";
    } else {
      const read = parseFootprints(footprint(web.pids), web.pids);
      const each = Object.values(read.byPid);
      footprintError = read.error;
      if (each.length > 0) {
        webContentFootprintKb = each.reduce((sum, one) => sum + one.footprintKb, 0);
        webContentFootprintPeakKb = each.reduce((most, one) => Math.max(most, one.peakKb), 0);
      }
    }
  }
  const harness = harnessMemory(rows, pid);
  return {
    webContentKb: web.rssKb,
    webContentProcesses: web.pids.length,
    webContentFootprintKb,
    webContentFootprintPeakKb,
    footprintError,
    harnessKb: harness.rssKb,
    harnessLargestKb: harness.largestKb,
  };
}

/** Adds one line of JSON to a file in `logs/`, for a run to be read back afterwards. */
export function logLine(file: string, record: object): void {
  mkdirSync(LOGS, { recursive: true });
  appendFileSync(join(LOGS, file), `${JSON.stringify({ at: Date.now(), ...record })}\n`);
}

/** Files in `dir` this run left behind whose name `wanted` accepts, newest last. */
function leftBehind(dir: string, wanted: (name: string) => boolean): string[] {
  try {
    return readdirSync(dir)
      .filter(wanted)
      .map((name) => join(dir, name))
      .filter((path) => statSync(path).mtimeMs >= RUN_STARTED)
      .sort((a, b) => statSync(a).mtimeMs - statSync(b).mtimeMs);
  } catch {
    return [];
  }
}

function run(program: string, args: string[]): string {
  try {
    return execFileSync(program, args, { encoding: "utf8", timeout: 20_000 });
  } catch (err) {
    const failed = err as { stdout?: string; message?: string };
    return failed.stdout || `(${program} ${args.join(" ")}: ${failed.message ?? "failed"})`;
  }
}

/**
 * Writes down what can still be learned after a test failed: whether the app is alive, how
 * big it was, whether the operating system killed it or recorded it crashing, and what a
 * panic hook wrote on its way out. Returns where it wrote.
 *
 * Called for every failed test, because a test that fails because the app died looks, from
 * the test, like any other: an element that never came.
 */
export function collectEvidence(app: string, why: string): string {
  mkdirSync(LOGS, { recursive: true });
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const report = join(LOGS, `evidence-${stamp}.txt`);
  const lines: string[] = [`what failed: ${why}`, `app: ${app}`];

  const pids = running(app);
  if (pids.length === 0) {
    lines.push("the app is NOT running: the process is gone, not only its WebDriver server");
  }
  for (const pid of pids) {
    lines.push(`the app is running as pid ${pid}: ${JSON.stringify(sample(pid))}`);
    lines.push("  so what stopped answering was inside the process, not the process itself");
  }
  lines.push(`fake harnesses running: ${harnessesRunning()}`);

  lines.push(
    existsSync(PANIC_LOG)
      ? `a panic was written down: see ${PANIC_LOG}`
      : "no panic was written down (a signal, or the operating system, ended it — or it lives)",
  );

  if (process.platform === "darwin") {
    // A crash leaves `charter-app-*.ips`; a kill for memory leaves `JetsamEvent-*.ips`, which
    // names the processes it killed only inside it — look for the app's name by file name
    // alone and a memory kill is missed.
    const reports = [
      join(homedir(), "Library", "Logs", "DiagnosticReports"),
      "/Library/Logs/DiagnosticReports",
    ].flatMap((dir) =>
      leftBehind(dir, (name) => name.startsWith("charter-app") || name.startsWith("JetsamEvent")),
    );
    for (const found of reports) {
      const kept = join(LOGS, found.split("/").pop() ?? "report.ips");
      try {
        copyFileSync(found, kept);
        lines.push(`crash or memory-kill report, kept as ${kept}`);
      } catch {
        lines.push(`crash or memory-kill report at ${found} (could not be copied)`);
      }
    }
    if (reports.length === 0) {
      lines.push("no crash report and no memory-kill report was written during this run");
    }
  } else if (process.platform === "linux") {
    // The kernel says when it killed something for memory, or when a process faulted.
    const kernel = run("sudo", ["-n", "dmesg", "--ctime"])
      .split("\n")
      .filter((line) => /oom|killed process|segfault|charter/i.test(line));
    lines.push("what the kernel said about memory kills and faults:");
    lines.push(...(kernel.length > 0 ? kernel.slice(-40) : ["(nothing)"]));
  }

  writeFileSync(report, `${lines.join("\n")}\n`);
  return report;
}
