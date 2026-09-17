/// <reference types="vite/client" />
import type { Terminal } from "@xterm/xterm";
import { drawWith, type Drawing, type Renderer } from "./renderer";

/**
 * The seam the benchmark measures the app through.
 *
 * Four of the spec's limits — keystroke to screen, tab and pane switch, the longest stall
 * during a burst, and the frame rate of a `?2026` animation — can only be seen from inside
 * the window: measured from the test process instead, every sample carries the driver's own
 * round trip, which is the same size as the budget being measured.
 *
 * So each pane hands its terminal to this module, and the benchmark asks it for work:
 * `begin()` starts a job and returns at once, `poll()` says whether it has finished and with
 * what. Nothing here awaits a driver, so the numbers are the app's own.
 *
 * **It exists only in the `e2e` build.** `attach()` is called from `main.tsx` and does
 * nothing unless `VITE_E2E` is set, which only `vite build --mode e2e` does
 * (`npm run build:e2e`); in every other build a pane's calls here return immediately.
 */

/** What a pane tells this module about itself. */
type Pane = {
  session: number;
  terminal: Terminal;
  drawing?: Drawing;
  /** Everything the pane has been sent since it was armed, in a window big enough to match
   *  a sentinel that arrives split across two chunks. */
  tail: string;
  chunks: number;
  characters: number;
  firstChunkAt?: number;
  /** Set while a job is waiting for text to be painted. `seen` is whether the text has
   *  arrived; the draw after that is the paint being timed. */
  waiting?: { needle: string; seen: boolean; found: (at: number) => void };
};

const TAIL = 4096;

const panes = new Map<number, Pane>();
/** Set by a `next pane` job: the next pane to open is the one the jobs after it measure. */
let awaited: ((session: number) => void) | undefined;
let subject: number | undefined;

/** A job runs on its own; `poll` is how the benchmark learns it has finished. Its failure is
 *  `trouble`, not `error`: WebdriverIO's Tauri plugin reads an answer with an `error` field
 *  as a failed call, and retries it. */
type Job = { running: boolean; result?: unknown; trouble?: string };
let job: Job = { running: false };

/** Whether this build has the seam at all. */
export const measuring = Boolean(import.meta.env.VITE_E2E);

/** A pane's terminal, now on screen. */
export function paneOpened(session: number, terminal: Terminal): void {
  if (!measuring) return;
  panes.set(session, { session, terminal, tail: "", chunks: 0, characters: 0 });
  terminal.onRender(() => {
    const pane = panes.get(session);
    if (!pane?.waiting?.seen) return;
    // The row is in the grid; one more frame presented is when a person sees it (research
    // §9.3, step 3).
    const { found } = pane.waiting;
    pane.waiting = undefined;
    requestAnimationFrame(() => found(performance.now()));
  });
  awaited?.(session);
}

/** A pane's terminal, gone from the screen. */
export function paneClosed(session: number): void {
  if (!measuring) return;
  panes.delete(session);
}

/** What the pane's renderer turned out to be. */
export function paneDrawing(session: number, drawing: Drawing): void {
  if (!measuring) return;
  const pane = panes.get(session);
  if (pane) pane.drawing = drawing;
}

/** Text a pane has been sent, before it writes it to its terminal. */
export function paneSent(session: number, text: string): void {
  if (!measuring) return;
  const pane = panes.get(session);
  if (!pane) return;
  pane.chunks += 1;
  pane.characters += text.length;
  pane.firstChunkAt ??= performance.now();
  if (!pane.waiting || pane.waiting.seen) return;
  pane.tail = (pane.tail + text).slice(-TAIL);
  // The text has arrived. The draw that follows is the paint of it, and `onRender` above
  // answers with the frame after that.
  if (pane.tail.includes(pane.waiting.needle)) pane.waiting.seen = true;
}

/** How long a rAF frame has been from the one before it, over a whole phase. One loop runs
 *  at a time: restarting watches from zero rather than adding a second loop. */
const frames = { loop: 0, from: 0, count: 0, longestGapMs: 0, last: 0 };

function watch(loop: number): FrameRequestCallback {
  const tick = (now: number) => {
    if (frames.loop !== loop) return;
    if (frames.last) frames.longestGapMs = Math.max(frames.longestGapMs, now - frames.last);
    frames.last = now;
    frames.count += 1;
    requestAnimationFrame(tick);
  };
  return tick;
}

/** The window's own buttons, pressed from inside it so a timed job waits for no driver. */
function button(name: string): HTMLElement {
  const all = [...document.querySelectorAll("button")];
  const found = all.find(
    (one) => one.getAttribute("aria-label") === name || one.textContent?.trim() === name,
  );
  if (!found) throw new Error(`no button called ${JSON.stringify(name)}`);
  return found;
}

/** The name of the tab in front. */
function selectedTab(): string | undefined {
  return (
    document.querySelector('[role="tab"][aria-selected="true"]')?.textContent?.trim() ?? undefined
  );
}

function pane(session?: number): Pane {
  const id = session ?? subject;
  const found = id === undefined ? undefined : panes.get(id);
  if (!found) throw new Error(`no pane is showing session ${id}`);
  return found;
}

/** Resolves with the moment `needle` was painted in `session`'s pane. */
function painted(session: number, needle: string): Promise<number> {
  return new Promise((found) => {
    const one = pane(session);
    one.tail = "";
    one.waiting = { needle, seen: false, found };
  });
}

function percentile(sorted: number[], at: number): number {
  if (sorted.length === 0) return NaN;
  const index = Math.min(sorted.length - 1, Math.floor((at / 100) * sorted.length));
  return sorted[index];
}

function summary(samples: number[]): Record<string, number> {
  const sorted = [...samples].sort((a, b) => a - b);
  return {
    samples: sorted.length,
    p50: percentile(sorted, 50),
    p99: percentile(sorted, 99),
    worst: sorted[sorted.length - 1],
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((done) => setTimeout(done, ms));
}

/** The jobs the benchmark can ask for. Each answers with numbers, never with an element. */
export type Plan =
  /** The next pane to open is what the jobs below measure. Then press `press`, if given. */
  | { kind: "next pane"; press?: string }
  /** Types `type`, if given — what releases a `--wait-for-input` load — then measures from
   *  the first text the pane is sent after that until `sentinel` is painted. `bytes` is how
   *  much the load writes, for a rate. */
  | { kind: "painted"; sentinel: string; bytes: number; type?: string; patienceMs?: number }
  /** `count` keystrokes, each waiting for the harness to echo it back, one at a time. */
  | { kind: "keystrokes"; count: number; session?: number }
  /** Press a tab, and measure until the pane it brings back has painted. */
  | { kind: "switch"; tab: string; sentinel: string }
  /** Split the pane in front, and measure until the new pane has painted. */
  | { kind: "split"; direction: "Split right" | "Split down"; sentinel: string }
  /** How many times the pane's terminal draws, over `ms`. */
  | { kind: "draw rate"; ms: number; session?: number };

async function work(plan: Plan): Promise<unknown> {
  switch (plan.kind) {
    case "next pane": {
      const opened = new Promise<number>((found) => {
        awaited = (session) => {
          awaited = undefined;
          subject = session;
          found(session);
        };
      });
      if (plan.press) button(plan.press).click();
      const session = await opened;
      return { session, tab: selectedTab() };
    }

    case "painted": {
      const one = pane();
      one.firstChunkAt = undefined;
      one.chunks = 0;
      one.characters = 0;
      const seen = painted(one.session, plan.sentinel);
      if (plan.type !== undefined) one.terminal.input(plan.type);
      const gave_up = sleep(plan.patienceMs ?? 120_000).then(() => undefined);
      const at = await Promise.race([seen, gave_up]);
      if (at === undefined) throw new Error(`${plan.sentinel} was never painted`);
      const from = one.firstChunkAt;
      if (from === undefined) throw new Error("the pane was sent nothing at all");
      const ms = at - from;
      return {
        ms,
        bytes: plan.bytes,
        megabytesPerSecond: plan.bytes / 1e6 / (ms / 1000),
        chunks: one.chunks,
        characters: one.characters,
      };
    }

    case "keystrokes": {
      const one = pane(plan.session);
      const samples: number[] = [];
      for (let n = 0; n < plan.count; n++) {
        const typed = `k${n}-${Math.random().toString(36).slice(2, 8)}`;
        const echo = `you said: ${typed}`;
        const seen = painted(one.session, echo);
        const from = performance.now();
        // What xterm.js does with a key press: `onData` fires, and the pane sends it on.
        one.terminal.input(`${typed}\r`);
        const at = await Promise.race([seen, sleep(20_000).then(() => undefined)]);
        if (at === undefined) throw new Error(`the harness never echoed ${typed}`);
        samples.push(at - from);
        // A person does not type at the speed of a loop.
        await sleep(25);
      }
      return { ...summary(samples), samples_ms: samples };
    }

    case "switch":
    case "split": {
      // A split redraws the pane it splits as well, so the pane being timed is the first one
      // to open for a session that was not already on screen.
      const showing = new Set(panes.keys());
      const opened = new Promise<number>((found) => {
        awaited = (session) => {
          if (showing.has(session)) return;
          awaited = undefined;
          subject = session;
          found(session);
        };
      });
      const from = performance.now();
      if (plan.kind === "switch") {
        const tab = [...document.querySelectorAll('[role="tab"]')].find(
          (one) => one.textContent?.trim() === plan.tab,
        );
        if (!tab) throw new Error(`no tab called ${plan.tab}`);
        (tab as HTMLElement).click();
      } else {
        button(plan.direction).click();
      }
      const session = await opened;
      const tab = selectedTab();
      const at = await Promise.race([
        painted(session, plan.sentinel),
        sleep(30_000).then(() => undefined),
      ]);
      if (at === undefined) throw new Error(`the pane never painted ${plan.sentinel}`);
      return { session, tab, ms: at - from };
    }

    case "draw rate": {
      const one = pane(plan.session);
      let draws = 0;
      const counting = one.terminal.onRender(() => {
        draws += 1;
      });
      const wasCount = frames.count;
      const from = performance.now();
      await sleep(plan.ms);
      const over = (performance.now() - from) / 1000;
      counting.dispose();
      return {
        seconds: over,
        draws,
        drawsPerSecond: draws / over,
        framesPerSecond: (frames.count - wasCount) / over,
      };
    }
  }
}

/** Everything the benchmark can ask of the window. */
export type Bench = {
  begin(plan: Plan): void;
  poll(): Job;
  /** Starts, or restarts, watching how long a frame takes. */
  watchFrames(): void;
  /** What the frames have done since `watchFrames`. */
  frames(): {
    seconds: number;
    count: number;
    longestGapMs: number;
    framesPerSecond: number;
    /** A window that is not visible draws no frames at all, which is no measurement. */
    visible: boolean;
  };
  /** Draws panes opened from now on with this renderer. */
  drawWith(kind: Renderer): void;
  /** The panes on screen: what each is drawing with, its size, and what it has been sent. */
  panes(): {
    session: number;
    renderer?: Renderer;
    active?: boolean;
    trouble?: string;
    columns: number;
    rows: number;
    chunks: number;
    characters: number;
  }[];
  /** Types into a pane, the way a key press does. */
  type(session: number, text: string): void;
  /** The session of the pane the last `next pane`, `switch` or `split` job opened. */
  subject(): number | undefined;
  /** Sets the pane's terminal — and so its session's program — to this size. */
  resize(session: number, columns: number, rows: number): void;
  /** Presses a button by its label or its text. */
  press(name: string): void;
  /** Clicks a pane, which makes it the focused one: the one the next split splits. */
  focus(session: number): void;
  /** The end of what a pane has been sent, for working out why a job is still waiting. */
  tail(session: number): { tail: string; waitingFor?: string; seen?: boolean };
};

const bench: Bench = {
  begin(plan) {
    if (job.running) throw new Error("a job is already running");
    job = { running: true };
    const mine = job;
    work(plan).then(
      (result) => {
        mine.running = false;
        mine.result = result;
      },
      (err: unknown) => {
        mine.running = false;
        mine.trouble = err instanceof Error ? err.message : String(err);
      },
    );
  },
  poll: () => ({ ...job }),
  watchFrames() {
    frames.loop += 1;
    frames.from = performance.now();
    frames.count = 0;
    frames.longestGapMs = 0;
    frames.last = 0;
    requestAnimationFrame(watch(frames.loop));
  },
  frames() {
    const seconds = (performance.now() - frames.from) / 1000;
    return {
      seconds,
      count: frames.count,
      longestGapMs: frames.longestGapMs,
      framesPerSecond: frames.count / seconds,
      visible: document.visibilityState === "visible",
    };
  },
  drawWith,
  panes: () =>
    [...panes.values()].map((one) => ({
      session: one.session,
      renderer: one.drawing?.renderer,
      active: one.drawing?.active,
      trouble: one.drawing?.trouble,
      columns: one.terminal.cols,
      rows: one.terminal.rows,
      chunks: one.chunks,
      characters: one.characters,
    })),
  type(session, text) {
    pane(session).terminal.input(text);
  },
  subject: () => subject,
  resize(session, columns, rows) {
    pane(session).terminal.resize(columns, rows);
  },
  press(name) {
    button(name).click();
  },
  tail(session) {
    const one = pane(session);
    return {
      tail: one.tail.slice(-400),
      waitingFor: one.waiting?.needle,
      seen: one.waiting?.seen,
    };
  },
  focus(session) {
    const found = document.querySelector(`[data-testid="pane"][data-session="${session}"]`);
    if (!found) throw new Error(`no pane is showing session ${session}`);
    (found as HTMLElement).click();
  },
};

declare global {
  interface Window {
    charterBench?: Bench;
  }
}

/** Puts the seam on `window`, in the `e2e` build and nowhere else. */
export function attach(): void {
  if (measuring) window.charterBench = bench;
}
