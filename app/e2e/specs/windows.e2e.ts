import { realpathSync, renameSync } from "node:fs";
import { dirname, join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { anEmptyRecord, copyFixturePlane } from "../harness.js";
import { attributesOfEach } from "../reading.js";

/**
 * A project tab split into an OS window of its own, and moved back (charter#126; ADR 0033,
 * amended 2026-09-26), in the built app.
 *
 * What only the built app can show: that a second window exists at all, that it is granted
 * the commands it needs (a split window with no grant draws nothing, ADR 0052), that it draws
 * the project it was handed, and that the project comes back — by the palette's row, and by
 * the window's close button — with the core holding it the whole time.
 *
 * **Every row is pressed from the palette**, in whichever window it is about: each window has
 * its own palette and its own `F2`, which is one of the three things #126 named.
 *
 * **It leaves the app as it found it**: back in the main window, with its own project closed by
 * `close_plane`, and its tab gone from the strip. One app process serves the whole run, and a
 * project left drawn here is the strip the next spec (`workspace-explorer.e2e.ts`) reads. The
 * core tells every window drawing a project it closed (`plane-closed`, #1242), so the tab goes
 * however the close was asked.
 */

const PROJECTS = '[role="tablist"][aria-label="Projects"]';
const PALETTE = '[role="dialog"][aria-label="Command palette"]';
const MAIN = "main";

/** A project of this spec's own, resolved the way charter spells it (`projects.e2e.ts`). */
const split = (() => {
  const copied = copyFixturePlane();
  const renamed = join(dirname(copied), "splitme");
  renameSync(copied, renamed);
  return realpathSync(renamed);
})();

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

/**
 * The paths of the project tabs in the window the driver is in, read in one pass: a move takes
 * a tab off this strip while it is being polled, and a tab found and then read can be gone in
 * between (charter#506, `reading.ts`).
 */
async function strip(): Promise<string[]> {
  return (await attributesOfEach(`${PROJECTS} [role="tab"]`, ["title"])).map(
    (tab) => tab.title ?? "",
  );
}

async function stripHas(path: string, has: boolean): Promise<void> {
  await browser.waitUntil(async () => (await strip()).includes(path) === has, {
    timeout: 30_000,
    timeoutMsg: `the project strip ${has ? "never showed" : "still shows"} ${path}`,
  });
}

/** What the driver says when the window it sent a command to is gone before it answered. */
const WINDOW_GONE = /No window could be found|Channel closed/;

/**
 * Runs one palette row by its title, in the window the driver is in.
 *
 * `closesThisWindow` is for a row whose whole effect is that this window goes. The `Enter` that
 * presses it is a WebDriver action, answered by the window it was sent to — and when the row
 * works, that window can be gone before it answers (`Channel closed`, then `No window could be
 * found` on the driver's own retry), which read as a failure of the very move that succeeded.
 * So for such a row only that answer is let go of, and the caller asserts the window went.
 */
async function fromThePalette(title: string, closesThisWindow = false): Promise<void> {
  await browser.keys(["F2"]);
  await $(PALETTE).waitForDisplayed({ timeout: 20_000 });
  const box = await $("#palette-query");
  await browser.waitUntil(async () => box.isFocused(), {
    timeout: 10_000,
    timeoutMsg: "the palette did not take the keyboard when it opened",
  });
  await box.addValue(title);
  await browser.waitUntil(
    async () =>
      browser.execute(
        (want: string) =>
          [...document.querySelectorAll('[role="option"]')].some(
            (row) => row.querySelector(".palette-title")?.textContent === want,
          ),
        title,
      ),
    { timeout: 10_000, timeoutMsg: `the palette never listed "${title}"` },
  );
  const pressed = browser.keys(["Enter"]);
  if (!closesThisWindow) return pressed;
  await pressed.catch((e: unknown) => {
    if (!WINDOW_GONE.test(String(e))) throw e;
  });
}

/** Waits for the window labelled `label` to exist, or to be gone. */
async function windowThere(label: string, there: boolean): Promise<void> {
  await browser.waitUntil(
    async () => (await browser.getWindowHandles()).includes(label) === there,
    {
      timeout: 30_000,
      timeoutMsg: `the window ${label} ${there ? "never appeared" : "never went"}`,
    },
  );
}

/** The split window charter made, which is every window but the main one. */
async function theSplitWindow(): Promise<string> {
  let found = "";
  await browser.waitUntil(
    async () => {
      found = (await browser.getWindowHandles()).find((one) => one !== MAIN) ?? "";
      return found !== "";
    },
    { timeout: 30_000, timeoutMsg: "charter never made a second window" },
  );
  return found;
}

describe("a project tab in a window of its own", function () {
  this.timeout(180_000);

  before(async () => {
    await browser.switchToWindow(MAIN);
    // Nothing to put back, so opening it starts nothing a later spec would inherit.
    anEmptyRecord(split);
    await $(`${PROJECTS} button[aria-label="Open a project…"]`).click();
    const box = await $("#open-by-path");
    await box.waitForDisplayed({ timeout: 20_000 });
    await box.addValue(split);
    await $("button=Open").click();
    const question = await $('[role="dialog"]');
    if (await question.waitForDisplayed({ timeout: 10_000 }).catch(() => false))
      await $("button=Open project").click();
    await stripHas(split, true);
  });

  after(async () => {
    // A test that failed half way can leave the project in a split window: that window's own
    // close request hands it back, as the last test proves.
    for (const label of (await browser.getWindowHandles()).filter((one) => one !== MAIN)) {
      await browser.switchToWindow(label);
      await browser
        .execute(() => {
          void window.__TAURI__.window.getCurrentWindow().close();
        })
        .catch(() => undefined);
      await windowThere(label, false);
    }
    await browser.switchToWindow(MAIN);
    // Let go of in the core, which tells the window to stop drawing it (#1242).
    if ((await ask<string[]>("open_planes")).includes(split))
      await ask("close_plane", { plane: split });
    await browser.waitUntil(async () => !(await ask<string[]>("open_planes")).includes(split), {
      timeout: 20_000,
      timeoutMsg: "the splitme project was not let go of",
    });
    await stripHas(split, false);
  });

  it("moves into a new window, which draws it, and leaves the main window", async () => {
    await fromThePalette("Move project splitme to a new window");

    const label = await theSplitWindow();
    await stripHas(split, false);
    // Still open in the core: moving a project ends nothing.
    expect(await ask<string[]>("open_planes")).toContain(split);

    await browser.switchToWindow(label);
    // Granted the commands it needs, or it could not have asked what it was handed.
    await stripHas(split, true);
    expect(await strip()).toEqual([split]);
  });

  it("moves back to the main window from the split window's own palette, which then goes", async () => {
    const label = await theSplitWindow();
    await browser.switchToWindow(label);

    await fromThePalette("Move project splitme to the main window", true);

    await windowThere(label, false);
    await browser.switchToWindow(MAIN);
    await stripHas(split, true);
  });

  it("comes back to the main window when its window is closed, with nothing ended", async () => {
    await browser.switchToWindow(MAIN);
    await fromThePalette("Move project splitme to a new window");
    const label = await theSplitWindow();
    await browser.switchToWindow(label);
    await stripHas(split, true);

    // The close button, as the operator presses it: a close REQUEST, which a split window
    // answers by handing its projects back.
    //
    // Not awaited inside the page: the window is gone before a script waiting on it could
    // answer, and the driver would report the window missing rather than the close done.
    await browser
      .execute(() => {
        void window.__TAURI__.window.getCurrentWindow().close();
      })
      .catch(() => undefined);

    await windowThere(label, false);
    await browser.switchToWindow(MAIN);
    await stripHas(split, true);
    expect(await ask<string[]>("open_planes")).toContain(split);
  });
});
