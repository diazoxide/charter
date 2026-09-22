import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import process from "node:process";
import { $, browser } from "@wdio/globals";
import type { Bench, Plan } from "../../src/bench.ts";
import type { Renderer } from "../../src/renderer.ts";
import { built } from "../harness.js";

/** What the benchmark measures in, and the pieces every spec shares. */

export const corpus = join(process.cwd(), "..", "fixtures", "corpora", "claude-code-session.raw");
export const CORPUS_BYTES = 132_146;

/** The frame research §9.3 measures in, and the size the corpus was recorded at. */
export const FRAME = { columns: 150, rows: 42 };

/** The renderer arm this run measures. `tools/bench.mjs` runs one arm, then the other. */
export const arm: Renderer = process.env.CHARTER_BENCH_RENDERER === "webgl" ? "webgl" : "dom";

/** Runs a job in the window and answers with its result, once it has finished. */
export async function job<T = Record<string, unknown>>(
  plan: Plan,
  patienceMs = 600_000,
): Promise<T> {
  // A person may have clicked another window since the last job.
  inFront();
  await browser.execute((plan) => (window.charterBench as Bench).begin(plan), plan);
  let state: { running: boolean; result?: unknown; trouble?: string } = { running: true };
  await browser.waitUntil(
    async () => {
      state = await browser.execute(() => (window.charterBench as Bench).poll());
      return !state.running;
    },
    { timeout: patienceMs, interval: 50, timeoutMsg: `${plan.kind} never finished` },
  );
  if (state.trouble) throw new Error(`${plan.kind}: ${state.trouble}`);
  return state.result as T;
}

/**
 * Brings the app's window in front of every other.
 *
 * WebKit draws nothing for a window that is covered — no animation frame, no terminal
 * render — so a job waiting for a paint in a covered window waits forever, and a frame count
 * reads zero. Every measurement that needs a paint is taken with the window in front.
 */
export function inFront(): void {
  if (process.platform !== "darwin") return;
  const { pid } = appProcess();
  execFileSync("osascript", [
    "-e",
    `tell application "System Events" to set frontmost of (first process whose unix id is ${pid}) to true`,
  ]);
}

/** How many frames a second the page has to be getting before anything here is measured. */
const FRAMES_EXPECTED = 50;

/**
 * Waits for the window's seam, checks the display is giving the page full frames, then draws
 * every pane from now on with this run's arm.
 *
 * Nothing can draw more often than the display changes, so a throttled display silently caps
 * every draw-rate and paint measurement in the run: a dimmed one gave 26 frames a second here
 * and made the DOM renderer look as if it drew 22. `caffeinate -u` in `tools/bench.mjs` holds
 * it at the rate it uses when someone is at the machine; this refuses to measure if it did not.
 */
export async function ready(): Promise<void> {
  inFront();
  await browser.waitUntil(async () => browser.execute(() => Boolean(window.charterBench)), {
    timeout: 30_000,
    timeoutMsg: "the window has no benchmark seam: was the frontend built with --mode e2e?",
  });
  await browser.execute((kind) => (window.charterBench as Bench).drawWith(kind), arm);

  await watchFrames();
  await sleep(1000);
  const drawn = await frames();
  if (drawn.framesPerSecond < FRAMES_EXPECTED) {
    throw new Error(
      `the page is getting ${drawn.framesPerSecond.toFixed(1)} frames a second, not the ` +
        `${FRAMES_EXPECTED}+ a measurement needs: the display is asleep, dimmed or throttled`,
    );
  }
}

export type PaneState = Awaited<ReturnType<Bench["panes"]>>[number];

export async function panes(): Promise<PaneState[]> {
  return browser.execute(() => (window.charterBench as Bench).panes());
}

export async function paneOf(session: number): Promise<PaneState> {
  const found = (await panes()).find((one) => one.session === session);
  if (!found) throw new Error(`no pane is showing session ${session}`);
  return found;
}

export async function subject(): Promise<number> {
  const session = await browser.execute(() => (window.charterBench as Bench).subject());
  if (session === undefined) throw new Error("no pane has been opened yet");
  return session;
}

/** Opens a tab, whose session runs the load chosen last, and answers with it and its tab. */
export async function openTab(): Promise<{ session: number; tab: string }> {
  return job<{ session: number; tab: string }>({ kind: "next pane", press: "New tab" });
}

/**
 * Sizes a pane to the benchmark frame, and waits until it has stayed that size: a pane that
 * has just opened fits itself to its space once its view is open, which would undo a resize
 * made before that.
 */
export async function frame(session: number): Promise<void> {
  let steady = 0;
  await browser.waitUntil(
    async () => {
      const pane = await paneOf(session);
      if (pane.columns === FRAME.columns && pane.rows === FRAME.rows) {
        steady += 1;
      } else {
        steady = 0;
        await browser.execute(
          (session, size) =>
            (window.charterBench as Bench).resize(session, size.columns, size.rows),
          session,
          FRAME,
        );
      }
      return steady >= 3;
    },
    {
      timeout: 20_000,
      interval: 150,
      timeoutMsg: `session ${session} never stayed ${FRAME.columns}x${FRAME.rows}`,
    },
  );
}

export async function type(session: number, text: string): Promise<void> {
  await browser.execute(
    (session, text) => (window.charterBench as Bench).type(session, text),
    session,
    text,
  );
}

export async function watchFrames(): Promise<void> {
  await browser.execute(() => (window.charterBench as Bench).watchFrames());
}

/** What the frames did since `watchFrames`. No frames at all is the window being covered,
 *  and is refused rather than reported as a number. */
export async function frames(): Promise<ReturnType<Bench["frames"]>> {
  const drawn = await browser.execute(() => (window.charterBench as Bench).frames());
  if (drawn.count === 0) {
    throw new Error("the window drew no frames at all: it was covered or hidden");
  }
  return drawn;
}

/** The app's own process: its pid and resident memory, asked of the operating system. */
export function appProcess(): { pid: number; rssMb: number } {
  const app = built(process.platform === "win32" ? "charter-app.exe" : "charter-app");
  const ps = execFileSync("ps", ["-A", "-o", "pid=,rss=,command="], { encoding: "utf8" });
  const mine = ps
    .split("\n")
    .map((line) => line.trim().match(/^(\d+)\s+(\d+)\s+(.*)$/))
    .filter((found): found is RegExpMatchArray => Boolean(found))
    .filter((found) => found[3].startsWith(app));
  if (mine.length !== 1) {
    throw new Error(`expected one ${app} running, found ${mine.length}`);
  }
  return { pid: Number(mine[0][1]), rssMb: Number(mine[0][2]) / 1024 };
}

/** How many fake harnesses are running, and how much CPU they are using between them. */
export function harnesses(): { running: number; cpuPercent: number } {
  const harness = built("fake-harness");
  const ps = execFileSync("ps", ["-A", "-o", "%cpu=,command="], { encoding: "utf8" });
  const lines = ps
    .split("\n")
    .map((line) => line.trim().match(/^([\d.]+)\s+(.*)$/))
    .filter((found): found is RegExpMatchArray => Boolean(found))
    // The program itself, not any command line that happens to mention it.
    .filter((found) => found[2].startsWith(harness));
  return {
    running: lines.length,
    cpuPercent: lines.reduce((sum, found) => sum + Number(found[1]), 0),
  };
}

/**
 * Closes every tab and waits until the harnesses are gone.
 *
 * A session nothing shows still streams into the core, so a test that leaves one running
 * hands the next test a load it does not know about and does not report.
 */
export async function closeEverything(): Promise<void> {
  // **One at a time, answering as it goes.** Ending a chat asks first, and the question is
  // modal — so the loop this used to be, clicking every close button in one `execute`, opened
  // one dialog and then pressed forty inert buttons behind it. The strip also collapses rather
  // than scrolling, so what it draws is not every tab: the loop goes round until the document
  // has no close button left rather than over a list read once.
  for (let pressed = 0; pressed < 200; pressed++) {
    const closer = await $('button[aria-label^="End chat "]');
    if (!(await closer.isExisting())) break;
    const name = (await closer.getAttribute("aria-label")) ?? "";
    await closer.click();
    const asking = await $('[role="alertdialog"]');
    await asking.waitForDisplayed({ timeout: 20_000 });
    await asking.$(`button=${name}`).click();
    await asking.waitForDisplayed({ timeout: 20_000, reverse: true });
  }
  await browser.waitUntil(async () => harnesses().running === 0, {
    timeout: 60_000,
    timeoutMsg: "closing every tab left harnesses running",
  });
}

export function sleep(ms: number): Promise<void> {
  return new Promise((done) => setTimeout(done, ms));
}

/** Where a spec's numbers go: one JSON file per spec and arm, merged by `tools/bench.mjs`. */
export function record(spec: string, results: Record<string, unknown>): void {
  const out = process.env.CHARTER_BENCH_OUT ?? join(process.cwd(), "..", "target", "bench");
  const file = join(out, `${arm}-${spec}.json`);
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, `${JSON.stringify({ arm, spec, results }, null, 2)}\n`);
  console.log(`charter-bench ${arm} ${spec}\n${JSON.stringify(results, null, 2)}`);
}
