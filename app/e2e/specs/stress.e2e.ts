import { readFileSync } from "node:fs";
import process from "node:process";
import { $, $$, browser, expect } from "@wdio/globals";
import { LAUNCHDS_LIMIT, READY, THE_APPS_STARTING_LIMIT, built } from "../harness.js";
import { answerTheAsk, pressAndStart } from "../opening.js";
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
 * The chats one device keeps open (ADR 0082 §3), all at once and every one of them hot: there
 * is no hibernation yet (SC-4), so this is harder than the target asks. A stress run starts
 * the app under launchd's open-file limit (`launchedUnderLaunchdsLimit`), and 200 chats hold
 * more descriptors than that, so they open only if the app raised its limit (SC-15).
 */
const OPEN_TARGET = 200;

/**
 * How long the whole spec may take — declared on the SUITE, and that is not a style choice.
 *
 * WebdriverIO does not leave the deadline to mocha. `executeAsync` in `@wdio/utils` reads
 * `this._runnable._timeout` ONCE, before the test body starts, and races the body against a
 * timer of its own that rejects with a bare `Error: Timeout`. A `this.timeout()` call in the
 * first line of the body moves mocha's runnable and arrives too late for that race, so the
 * spec ran under `mochaOpts.timeout` — 180 s — while its own first line asked for 900.
 *
 * Measured on `main` on 2026-09-20, the whole body end to end: macOS 170 s in the one run of
 * six that passed and Linux 128 s in the same run. The five macOS runs that did not finish
 * would have taken 176, 181, 200, 212 and 218 s — extrapolations, each at its own run's
 * measured round cost, because the timeout cut the trace short. The 180 s budget sat inside
 * that spread, so it decided the result and the app never did.
 *
 * The first run of this spec with the budget where WebdriverIO reads it settles it: 225 s on
 * macOS, past the old budget, with the app healthy throughout — 219/217/217 threads with
 * fifty open and 14/13/14 once they were closed, one process, 194–213 MB. Under the old
 * spelling that run is a sixth red X and nothing to show for it.
 *
 * 900 s is the backstop of last resort, not a limit anything is expected to approach: it is
 * four times the slowest macOS run measured, and every wait inside the body has a deadline
 * and a message of its own that fires long before it. `ROUND_BUDGET` bounds the one loop
 * that had none.
 *
 * `budget.test.ts` keeps this on the suite, because the mistake is invisible from the spec.
 */
const BUDGET = 15 * 60_000;

/**
 * How long one round of fifty may take. The slowest round measured on a macOS runner on
 * 2026-09-20 was 74 s (run 35533226573, round 2); this is over three times that. It exists
 * so that fifty opens that stop making progress are reported as the tab they stopped on
 * rather than as `Error: Timeout` with nothing after it.
 */
const ROUND_BUDGET = 4 * 60_000;

/**
 * How long opening the two hundred may take. Each test runs under the suite's `BUDGET`, and the
 * close that follows needs room too (100 s on a macOS runner on 2026-10-01), so this is under
 * it with minutes to spare: a guard past the budget would never fire, and the test would end
 * as a bare `Error: Timeout` again. The opens took 184 s on macOS and 137 s on Ubuntu, so this
 * is over three times the slower one.
 */
const OPEN_TARGET_BUDGET = 10 * 60_000;

/** How often the round writes down how far it has got, in tabs. */
const PROGRESS_EVERY = 10;

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
      '[role="tablist"][aria-label="Tabs"] button[aria-label^="End chat "]',
    ).getElements()),
  ];
}

/** The workspaces on the strip above the tabs (ADR 0036). */
async function workspaceTabs(): Promise<WebdriverIO.Element[]> {
  return [...(await $$('[role="tablist"][aria-label="Workspaces"] [role="tab"]').getElements())];
}

/** What a strip tab is called: its drawn name, or — the plane root's icon tab (SI-1) — its
 *  accessible one. */
async function tabName(tab: WebdriverIO.Element): Promise<string> {
  const drawn = await tab.$(".workspace-name");
  return (await drawn.isExisting())
    ? drawn.getText()
    : ((await tab.getAttribute("aria-label")) ?? "");
}

/** Focuses a workspace by the name on its strip tab. */
async function focusWorkspace(name: string): Promise<boolean> {
  for (const tab of await workspaceTabs()) {
    if ((await tabName(tab)) === name) {
      await tab.click();
      return true;
    }
  }
  return false;
}

/** The show-more button on the chat strip, when the strip is hiding anything. */
async function hiddenTabs(): Promise<number> {
  const more = await $('.bar button[aria-label^="Show "]');
  if (!(await more.isExisting())) return 0;
  const said = await more.getAttribute("aria-label");
  return Number(/^Show (\d+) tabs? /.exec(said ?? "")?.[1] ?? 0);
}

/**
 * Closes every tab, last first, and waits for every session's program to be gone.
 *
 * **Every workspace's**, not the focused one's. The chat strip shows one workspace's chats
 * (ADR 0036), and one app process serves the whole run — so a chat another spec left in
 * another workspace is a live harness this spec would otherwise count as a leak.
 *
 * **The strip collapses rather than scrolling** (ADR 0039, as amended), so at fifty
 * chats it draws a handful and the rest are behind the show-more button. This loop still
 * terminates, and the reason is worth writing down because it is the whole of the
 * reachability argument: closing a drawn tab gives the strip room for a hidden one, so the
 * hidden tabs flow onto the strip as the drawn ones go. `4 * most` presses is four times what
 * `most` chats need. What would catch it going wrong is the pair of assertions below — no
 * close buttons AND nothing hidden — rather than the loop running out, and then the wait for
 * `harnessesRunning() === 0` underneath.
 */
async function closeEveryTab(most = TABS): Promise<void> {
  const names: string[] = [];
  for (const tab of await workspaceTabs()) names.push(await tabName(tab));
  for (const name of names.length > 0 ? names : [""]) {
    if (name !== "" && !(await focusWorkspace(name))) continue;
    // Bounded, so a close that never takes shows up as a failure and not as a hang.
    for (let pressed = 0; pressed < 4 * most; pressed++) {
      const buttons = await closeButtons();
      if (buttons.length === 0) {
        break;
      }
      // **Answered, not only pressed.** Ending a chat asks first now, and the question is
      // modal: a second press with the dialog up reaches nothing, so a loop that skipped the
      // answer would spend its whole budget clicking an inert strip.
      const closing = buttons[buttons.length - 1];
      const ending = (await closing.getAttribute("aria-label")) ?? "";
      await closing.click();
      await answerTheAsk(ending);
    }
    expect(await closeButtons()).toHaveLength(0);
    // **And nothing left behind the button**, which is the assertion the collapse added. An
    // empty strip with forty chats still hidden would otherwise read as "all closed" here and
    // only fail sixty seconds later, as a leak, with no sign of what leaked.
    expect(await hiddenTabs()).toBe(0);
  }
  // On a known workspace, so the fifty this spec is about all land on one strip — a real one,
  // not the plane root first on the strip (SI-1): the app process is shared with the specs
  // after this one, and they expect a workspace's panels in front.
  const workspace = names.find((name) => name !== "Plane root");
  if (workspace !== undefined) await focusWorkspace(workspace);
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

describe("fifty tabs over and over, then two hundred at once", function () {
  // Three rounds of fifty, and then two hundred, are minutes each, not the seconds a scenario
  // usually takes. On the suite rather than in a test body: see BUDGET. It is each test's.
  this.timeout(BUDGET);

  it("opens fifty tabs with no pause, closes them all, three times, alive and usable throughout", async () => {
    const pid = theApp();
    await closeEveryTab();
    const began = Date.now();
    look(pid, 0, "nothing open", began);

    const afterClosing: Sample[] = [];
    for (let round = 1; round <= ROUNDS; round++) {
      const roundBegan = Date.now();
      // No pause: the next tab is asked for as soon as the last one's picker has gone. Every
      // tenth tab is written down, because a round that runs out of time has to say how far
      // it got: the five timeouts of 2026-09-20 all landed inside this loop, and the trace
      // they left stopped at the previous round's "all closed" — 50 tabs earlier.
      for (let opened = 0; opened < TABS; opened++) {
        await pressAndStart("New tab");
        if ((opened + 1) % PROGRESS_EVERY === 0) {
          look(pid, round, `${opened + 1} of ${TABS} open`, began);
        }
        const spent = Date.now() - roundBegan;
        if (spent > ROUND_BUDGET) {
          throw new Error(
            `round ${round}: ${opened + 1} of ${TABS} tabs took ${spent / 1000}s, past the ` +
              `${ROUND_BUDGET / 1000}s a round is given (see logs/stress.jsonl)`,
          );
        }
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

  it("opens two hundred chats in an app started under launchd's open-file limit", async () => {
    // What the launcher started the app with. Not the app's limit now, which nothing outside
    // it can read on macOS: the app raises its own as it starts.
    let startedWith = Number.NaN;
    try {
      startedWith = Number(readFileSync(THE_APPS_STARTING_LIMIT, "utf8").trim());
    } catch {
      // Not written: the app was not started through the launcher. Said below.
    }
    if (!(startedWith <= LAUNCHDS_LIMIT)) {
      throw new Error(
        `the app was started with a soft open-file limit of ${startedWith}, so this cannot ` +
          `show it gets past launchd's ${LAUNCHDS_LIMIT}: a stress run (STRESS=1) starts it ` +
          "through launchedUnderLaunchdsLimit",
      );
    }
    const pid = theApp();
    await closeEveryTab();
    const began = Date.now();
    look(pid, 0, `${OPEN_TARGET}: nothing open, started with a limit of ${startedWith}`, began);

    for (let opened = 0; opened < OPEN_TARGET; opened++) {
      await pressAndStart("New tab");
      if ((opened + 1) % PROGRESS_EVERY === 0) {
        look(pid, 0, `${opened + 1} of ${OPEN_TARGET} open`, began);
      }
      const spent = Date.now() - began;
      if (spent > OPEN_TARGET_BUDGET) {
        throw new Error(
          `${opened + 1} of ${OPEN_TARGET} tabs took ${spent / 1000}s, past the ` +
            `${OPEN_TARGET_BUDGET / 1000}s they are given (see logs/stress.jsonl)`,
        );
      }
    }
    await browser.waitUntil(async () => harnessesRunning() === OPEN_TARGET, {
      timeout: 120_000,
      interval: 250,
      timeoutMsg: `${harnessesRunning()} of ${OPEN_TARGET} sessions ever ran`,
    });
    const open = look(pid, 0, `${OPEN_TARGET} open`, began);
    // The app holds more descriptors than it was started with, which it can only do because
    // it raised its limit. Without the count this test would pass on a host that opened its
    // chats some cheaper way and prove nothing about the limit.
    expect(open.descriptors).not.toBeNull();
    expect(open.descriptors ?? 0).toBeGreaterThan(startedWith);
    await answers(`with ${OPEN_TARGET} open`);

    await closeEveryTab(OPEN_TARGET);
    look(pid, 0, `${OPEN_TARGET}: all closed`, began);
    expect(theApp()).toBe(pid);
  });
});
