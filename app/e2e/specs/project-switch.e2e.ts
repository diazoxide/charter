import { browser, expect } from "@wdio/globals";
import type { Bench, Plan } from "../../src/bench.ts";
import { READY, built, declareAProfile, writeShell } from "../harness.js";
import { endChat, pressAndStart } from "../opening.js";
import { logLine } from "../processes.js";
import { attributesOfEach } from "../reading.js";
import {
  BUDGET_MS,
  OPEN,
  aProjectOfItsOwn,
  closeTheProjectsOfItsOwn,
  inFront,
  measureSwitches,
  openFromTheStrip,
} from "../switching.js";

/**
 * **Ten projects open, and each brought to the front through the switcher** (FR-27), in the
 * built app, timed — the evidence row L9 of the spec's budgets of record reads on every pull
 * request.
 *
 * Each project has one chat open, as the operator holds them, and each switch is timed inside
 * the window (`bench.ts`, `project switch`): from the press of the project's row until the pane
 * of its chat has painted, which is when a person sees the project they asked for. Timed from
 * the test process, every sample would carry the driver's own round trip, which is the size of
 * the budget.
 *
 * **What it asserts is that every switch arrives, not how fast.** A timing held absolutely on a
 * shared runner is ADR 0086's short list of exceptions, and this row is not on it: L9 is release
 * absolute (`bench/projects.bench.ts`, on the operator's machine) and CI relative once SC-16's
 * `bench` job exists. Until then the numbers are written down — `logs/project-switch.jsonl` and
 * the job's own output — beside the budget they are for.
 *
 * **It leaves the window as it found it.** Specs share one app process and run in name order:
 * the nine projects it opened are closed, the chat it opened in the launch's project is ended,
 * and the project the launch opened is in front again.
 */

const TABS = '[role="tablist"][aria-label="Tabs"]';

/** How many times each project is switched to. */
const ROUNDS = 3;

/** What the app answered a command with, insisting it answered at all. */
async function ask<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const answer = await browser.executeAsync(
    (
      name: string,
      passed: Record<string, unknown>,
      done: (out: { ok?: unknown; trouble?: string }) => void,
    ) => {
      void window.__TAURI__.core
        .invoke(name, passed)
        .then((ok) => done({ ok }))
        .catch((e: unknown) => done({ trouble: String(e) }));
    },
    command,
    args,
  );
  if (answer.trouble !== undefined) throw new Error(`${command} refused: ${answer.trouble}`);
  return answer.ok as T;
}

/** Runs a job in the window and answers with its result, once it has finished. */
async function job<T>(plan: Plan): Promise<T> {
  await browser.execute((plan) => (window.charterBench as Bench).begin(plan), plan);
  let state: { running: boolean; result?: unknown; trouble?: string } = { running: true };
  await browser.waitUntil(
    async () => {
      state = await browser.execute(() => (window.charterBench as Bench).poll());
      return !state.running;
    },
    { timeout: 60_000, interval: 50, timeoutMsg: `${plan.kind} never finished` },
  );
  if (state.trouble) throw new Error(`${plan.kind}: ${state.trouble}`);
  return state.result as T;
}

/** How many chat tabs the project in front is showing. */
async function chatTabs(): Promise<number> {
  return (await attributesOfEach(`${TABS} [role="tab"]`, ["role"])).length;
}

/** The names of the project in front's tabs, read in one pass (`footer.e2e.ts`'s). */
async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [
      ...(document
        .querySelector('[role="tablist"][aria-label="Tabs"]')
        ?.querySelectorAll('[role="tab"]') ?? []),
    ].map((tab) => tab.textContent ?? ""),
  );
}

/** Opens one chat in the project in front, and waits for its tab. */
async function aChatInFront(): Promise<void> {
  const before = await chatTabs();
  await pressAndStart("New tab");
  await browser.waitUntil(async () => (await chatTabs()) > before, {
    timeout: 30_000,
    timeoutMsg: "the chat this spec opened never got a tab",
  });
}

describe(`switching among ${OPEN} open projects`, function () {
  // On the describe, where WebdriverIO reads it (`e2e/budget.test.ts`). Nine opens through the
  // trust gate and nine chats are most of it; the thirty switches are seconds.
  this.timeout(600_000);

  /** The project the launch opened, which this spec never closes. */
  let first = "";
  /** Its tabs before this spec opened a chat there, so only that chat is ended again. */
  let wereAlreadyOpen: string[] = [];
  /** The window's size before this spec made room for ten project tabs. */
  let size = { width: 0, height: 0 };
  const planes: string[] = [];

  before(async () => {
    size = await browser.getWindowSize();
    await browser.setWindowSize(1600, 1000);
    first = (await ask<string[]>("open_planes"))[0];
    planes.push(first);
    // A chat of this spec's own in front, always: whatever an earlier spec left in front there
    // may be a view, which has no pane to paint.
    wereAlreadyOpen = await tabNames();
    await aChatInFront();
    for (let n = 1; n < OPEN; n++) {
      // The profile every scenario chat runs, so each project's chat is the fake harness
      // printing `READY`.
      const plane = aProjectOfItsOwn(n, (at) =>
        declareAProfile(at, writeShell(built("fake-harness"))),
      );
      await openFromTheStrip(plane);
      await aChatInFront();
      planes.push(plane);
    }
    expect(await ask<string[]>("open_planes")).toHaveLength(OPEN);
  });

  after(async () => {
    await closeTheProjectsOfItsOwn();
    expect(await ask<string[]>("open_planes")).toEqual([first]);
    await browser.waitUntil(async () => (await inFront()) === first, {
      timeout: 30_000,
      timeoutMsg: "the launch's project did not come back to the front",
    });
    for (const name of (await tabNames()).filter((tab) => !wereAlreadyOpen.includes(tab))) {
      await endChat(`End chat ${name}`);
    }
    await browser.setWindowSize(size.width, size.height);
  });

  it("brings each one to the front through the switcher, and its chat's pane paints", async () => {
    // In the order they were opened: the one in front is the last one opened, so no switch is
    // ever to the project already there.
    const switches = await measureSwitches(planes, ROUNDS, () => READY, job);

    logLine("project-switch.jsonl", { budgetMs: BUDGET_MS, ...switches });
    // In the job's own output too, where a reviewer reads it without downloading anything.
    console.log(`L9 project switch, ${BUDGET_MS} ms budget: ${JSON.stringify(switches)}`);

    expect(switches.samples).toBe(OPEN * ROUNDS);
  });
});
