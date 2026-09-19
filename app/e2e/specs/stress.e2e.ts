import process from "node:process";
import { $, $$, browser, expect } from "@wdio/globals";
import { READY, built } from "../harness.js";
import { pressAndStart } from "../opening.js";
import { type Sample, harnessesRunning, logLine, running, sample } from "../processes.js";

/**
 * Fifty tabs as fast as they open, every one closed, and again: the load the app exists for,
 * done harder than a person does it, on every scenario run (charter-app#16).
 *
 * The app died once while a scenario was opening its fiftieth tab, and has not since on the
 * machine it died on. CI runs on quiet machines, so this gives that death its chances where it
 * will be recorded: three rounds of fifty with no pause between tabs, the app's memory and
 * thread count written to `logs/stress.jsonl` at every step, and — if the app does die — the
 * evidence `wdio.conf.ts` writes down for every failed test.
 *
 * It also holds the app to what closing a tab is for. Closing ends the session's program and
 * every thread the session ran, so fifty tabs closed leave the app no bigger in threads than
 * fifty tabs closed did the first time.
 */

const ROUNDS = 3;
const TABS = 50;

/**
 * Threads the app may gain between the first round's close and the last's with nothing
 * leaking: its runtime's pools grow as they are used. A session runs at least three threads
 * of its own, so one leaked per tab is 150 across a round — far past this.
 */
const THREAD_SLACK = 40;

const app = built(process.platform === "win32" ? "charter-app.exe" : "charter-app");

/** The app under test, which is one process for the whole run. */
function theApp(): number {
  const pids = running(app);
  if (pids.length !== 1) {
    throw new Error(`expected one app process, found ${pids.length}: ${pids.join(", ")}`);
  }
  return pids[0];
}

/** The close buttons in the tab bar, left to right. */
async function closeButtons(): Promise<WebdriverIO.Element[]> {
  return [
    ...(await $$(
      '[role="tablist"][aria-label="Tabs"] button[aria-label^="Close tab "]',
    ).getElements()),
  ];
}

/** Closes every tab, last first, and waits for every session's program to be gone. */
async function closeEveryTab(): Promise<void> {
  // Bounded, so a close that never takes shows up as a failure and not as a hang.
  for (let pressed = 0; pressed < 4 * TABS; pressed++) {
    const buttons = await closeButtons();
    if (buttons.length === 0) {
      break;
    }
    await buttons[buttons.length - 1].click();
  }
  expect(await closeButtons()).toHaveLength(0);
  await browser.waitUntil(async () => harnessesRunning() === 0, {
    timeout: 60_000,
    interval: 250,
    timeoutMsg: `every tab closed, and ${harnessesRunning()} sessions were still running`,
  });
}

/**
 * A look at the app, written down; the test fails here if the app is gone. Printed as well as
 * kept, because CI keeps `logs/` only for a run that failed, and the numbers are wanted from
 * the runs that did not.
 */
function look(pid: number, round: number, step: string, began: number): Sample {
  const seen = sample(pid);
  if (seen === null) {
    throw new Error(`the app (pid ${pid}) is gone: round ${round}, ${step}`);
  }
  const record = {
    round,
    step,
    seconds: (Date.now() - began) / 1000,
    harnesses: harnessesRunning(),
    ...seen,
  };
  logLine("stress.jsonl", record);
  console.log(`charter-stress ${JSON.stringify(record)}`);
  return seen;
}

/**
 * Waits for the app's thread count to stop falling. A closed session's threads end once its
 * program is reaped, which waits out the program's grace to leave on its own; the runtime's
 * idle pool threads go only after about ten seconds. Counting before either would read a
 * burst still draining as a leak.
 */
async function settle(pid: number): Promise<void> {
  const threads = () => sample(pid)?.threads ?? 0;
  let last = threads();
  let steady = 0;
  const giveUp = Date.now() + 30_000;
  while (steady < 3 && Date.now() < giveUp) {
    await browser.pause(2_000);
    const now = threads();
    steady = now >= last ? steady + 1 : 0;
    last = now;
  }
}

/** The pane in front answers what is typed into it. See `panes.e2e.ts` for why the text goes
 *  to the terminal's own input rather than to the window. */
async function answers(said: string): Promise<void> {
  const pane = await $('[data-testid="pane"]');
  const showing = async () => (await pane.$(".xterm-rows").getText()).replace(/\s+/g, " ");
  await browser.waitUntil(async () => (await showing()).includes(READY), {
    timeout: 30_000,
    interval: 250,
    timeoutMsg: "the pane in front never showed its session was ready",
  });
  await pane.click();
  await pane.$(".xterm-helper-textarea").addValue(`${said}\n`);
  await browser.waitUntil(async () => (await showing()).includes(`you said: ${said}`), {
    timeout: 30_000,
    interval: 250,
    timeoutMsg: `the pane in front never answered ${JSON.stringify(said)}`,
  });
}

describe("fifty tabs, over and over", () => {
  it("opens fifty tabs with no pause, closes them all, three times, alive and usable throughout", async function () {
    // Three rounds of fifty is minutes, not the seconds a scenario usually takes.
    this.timeout(15 * 60_000);
    const pid = theApp();
    await closeEveryTab();
    const began = Date.now();
    look(pid, 0, "nothing open", began);

    const afterClosing: Sample[] = [];
    for (let round = 1; round <= ROUNDS; round++) {
      // No pause: the next tab is asked for as soon as the last one's picker has gone.
      for (let opened = 0; opened < TABS; opened++) {
        await pressAndStart("New tab");
      }
      await browser.waitUntil(async () => harnessesRunning() === TABS, {
        timeout: 120_000,
        interval: 250,
        timeoutMsg: `round ${round}: ${harnessesRunning()} of ${TABS} sessions ever ran`,
      });
      look(pid, round, "fifty open", began);
      await answers(`round ${round} with fifty open`);

      await closeEveryTab();
      await settle(pid);
      afterClosing.push(look(pid, round, "all closed", began));
    }

    expect(theApp()).toBe(pid);
    const grew = afterClosing[ROUNDS - 1].threads - afterClosing[0].threads;
    if (grew > THREAD_SLACK) {
      throw new Error(
        `the app kept ${grew} more threads after round ${ROUNDS}'s tabs closed than after ` +
          `round 1's: closing is leaking what a session runs (see logs/stress.jsonl)`,
      );
    }
  });
});
