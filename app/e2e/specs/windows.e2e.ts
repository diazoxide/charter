import { realpathSync, renameSync } from "node:fs";
import { dirname, join } from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import { anEmptyRecord, copyFixturePlane } from "../harness.js";

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
 * **It leaves the app as it found it**: back in the main window, with its own project closed.
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

/** The paths of the project tabs in the window the driver is in. */
async function strip(): Promise<string[]> {
  const tabs = await $$(`${PROJECTS} [role="tab"]`).getElements();
  const paths = [];
  for (const tab of tabs) paths.push((await tab.getAttribute("title")) ?? "");
  return paths;
}

async function stripHas(path: string, has: boolean): Promise<void> {
  await browser.waitUntil(async () => (await strip()).includes(path) === has, {
    timeout: 30_000,
    timeoutMsg: `the project strip ${has ? "never showed" : "still shows"} ${path}`,
  });
}

/** Runs one palette row by its title, in the window the driver is in. */
async function fromThePalette(title: string): Promise<void> {
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
  await browser.keys(["Enter"]);
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
    await browser.switchToWindow(MAIN);
    await ask("close_plane", { plane: split }).catch(() => undefined);
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

    await fromThePalette("Move project splitme to the main window");

    await windowThere(label, false);
    await browser.switchToWindow(MAIN);
    await stripHas(split, true);
  });

  it("comes back to the main window when its window is closed, with nothing ended", async () => {
    await fromThePalette("Move project splitme to a new window");
    const label = await theSplitWindow();
    await browser.switchToWindow(label);
    await stripHas(split, true);

    // The close button, as the operator presses it: a close REQUEST, which a split window
    // answers by handing its projects back.
    await browser.executeAsync((done: () => void) => {
      void window.__TAURI__.window.getCurrentWindow().close().finally(done);
    });

    await windowThere(label, false);
    await browser.switchToWindow(MAIN);
    await stripHas(split, true);
    expect(await ask<string[]>("open_planes")).toContain(split);
  });
});
