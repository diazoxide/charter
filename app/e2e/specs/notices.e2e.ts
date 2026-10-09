import { existsSync, realpathSync, renameSync } from "node:fs";
import { dirname, join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { anEmptyRecord, copyFixturePlane } from "../harness.js";
import { bandNotice } from "../reading.js";

/**
 * **A pin to a workspace that is gone is kept dormant** (NO-1 #1223, ruling V91c as amended),
 * in the built app against the real core and the real machine store.
 *
 * `app/src/Pins.test.tsx` drives the same thing against a mocked core. What only a real window
 * can show is the whole path: a workspace directory going away on disk, the project watcher
 * telling the window, the Notice, the store **still holding the pin** — read back from the core,
 * never from the strip — and the workspace coming back drawn in its own place.
 *
 * **Its own project, copied into the run's tree**, as `workspace-lifecycle.e2e.ts` does: this
 * spec moves a workspace directory away, so it is never pointed at the project the launch
 * opened. **It restores what it changed** — the directory, this project's pins in the store, and
 * the window, which lets the project go through its `×` — because one app process serves the
 * whole run and a spec that leaked state broke train 38.
 */

const PROJECTS = '[data-strip="Projects"]';

type Pins = { project: boolean; workspaces: string[]; missing: string[]; order: string[] };

const NAME = "dormant-pins";

/** This spec's own project: the fixture plane, with nothing to put back on open. */
const mine = (() => {
  const copied = copyFixturePlane();
  const renamed = join(dirname(copied), NAME);
  renameSync(copied, renamed);
  const root = realpathSync(renamed);
  anEmptyRecord(root);
  return root;
})();

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

const pins = () => ask<Pins>("plane_pins", { plane: mine });
const pinOrder = async () => (await pins()).order;

/** The workspaces the strip is showing, left to right, after the project root's tab. */
async function stripNames(): Promise<string[]> {
  return browser.execute(
    (selector: string) =>
      [
        ...(document.querySelector(selector)?.querySelectorAll('[role="tab"]:not(.plane-root)') ??
          []),
      ].map((tab) => tab.querySelector(".workspace-name")?.textContent ?? ""),
    '[data-strip="Workspaces"]',
  );
}

describe("a pin to a workspace that is gone", function () {
  this.timeout(180_000);

  /** The project the launch opened, which this spec never touches. */
  let first = "";
  /** This project's pins before the spec changed any. */
  let found: string[] = [];
  /** The workspace moved away, and where it went. */
  let gone = "";
  const away = join(dirname(mine), "notices-moved-away");

  before(async () => {
    first = (await ask<string[]>("open_planes"))[0];
    // Through the opener, as `workspace-lifecycle.e2e.ts` opens its own project and for its
    // reason: a window learns it holds a project by opening one.
    await $(`${PROJECTS} button[aria-label="Open a project…"]`).click();
    const box = await $("#open-by-path");
    await box.waitForDisplayed({ timeout: 20_000 });
    await box.addValue(mine);
    await $("button=Open").click();
    const question = await $('[role="dialog"]');
    await question.waitForDisplayed({ timeout: 30_000 });
    await $("button=Open project").click();
    await browser.waitUntil(async () => (await ask<string[]>("open_planes")).includes(mine), {
      timeout: 30_000,
      timeoutMsg: "this spec's project never opened",
    });
    found = await pinOrder();
  });

  after(async () => {
    // The directory back, then the pins as they were found, then the window as it was found.
    if (gone !== "" && existsSync(away)) renameSync(away, join(mine, "workspaces", gone));
    const now = await pinOrder();
    for (const name of now.filter((one) => !found.includes(one)))
      await ask("pin_workspace", { plane: mine, workspace: name, pinned: false });
    for (const name of found.filter((one) => !now.includes(one)))
      await ask("pin_workspace", { plane: mine, workspace: name, pinned: true });
    await ask("arrange_workspace_pins", { plane: mine, workspaces: found });
    expect(await pinOrder()).toEqual(found);
    // Let go of in the core, as `view-tabs.e2e.ts` does: the window is told and takes the tab
    // out (`plane-closed`, #1242).
    const selector = `${PROJECTS} button[aria-label="Close project ${NAME}"]`;
    if ((await ask<string[]>("open_planes")).includes(mine))
      await ask("close_plane", { plane: mine });
    await browser.waitUntil(async () => !(await $(selector).isExisting()), {
      timeout: 20_000,
      timeoutMsg: "this spec's project's tab stayed on the strip",
    });
    await browser.waitUntil(async () => !(await ask<string[]>("open_planes")).includes(mine), {
      timeout: 20_000,
      timeoutMsg: "this spec's project was not let go of",
    });
    expect((await ask<string[]>("open_planes"))[0]).toBe(first);
  });

  it("is kept dormant while its workspace is gone, and drawn in its place when it is back", async () => {
    // Two pins, so coming back FIRST is not what pinning alone would do (it appends).
    for (const name of ["alpha", "beta"]) {
      if (!(await pinOrder()).includes(name))
        await ask("pin_workspace", { plane: mine, workspace: name, pinned: true });
    }
    const before = await pinOrder();
    gone = before[0];
    let saw: string[] = [];
    await browser.waitUntil(
      async () => {
        saw = await stripNames();
        return JSON.stringify(saw.slice(0, before.length)) === JSON.stringify(before);
      },
      { timeout: 30_000, timeoutMsg: `the strip stayed ${JSON.stringify(saw)}` },
    );

    renameSync(join(mine, "workspaces", gone), away);

    // Found wherever the band put it: another Notice standing — a finding about the runner's
    // machine — may sort first and put this one behind "+N more" (#1250).
    const cause = `pin-dormant:${gone}`;
    let said = "";
    await browser.waitUntil(
      async () => {
        const now = await bandNotice(cause);
        if (now.state === "shown") said = now.text;
        return now.state === "shown";
      },
      { timeout: 30_000, timeoutMsg: `no Notice said ${gone} is gone` },
    );
    expect(said).toContain(`${gone} is gone, kept dormant`);
    await expect($(`[data-cause="${cause}"]`).$("button=Forget")).toBeExisting();

    // **A line under the strip wraps, whatever it says** (#1497): a reason with a path in it,
    // which has nowhere to break, is drawn inside the window and on more lines, never past
    // the window's edge. Measured on this Notice's own box with a long word put in its
    // sentence for the measurement, and taken out again: one a real refusal would carry.
    const wrapped = await browser.execute((wanted: string) => {
      const line = [...document.querySelectorAll<HTMLElement>("[data-cause]")].find(
        (one) => one.dataset.cause === wanted,
      );
      const says = line?.querySelector<HTMLElement>(".notice-says");
      if (line === undefined || says === null || says === undefined) return undefined;
      const short = line.getBoundingClientRect().height;
      const word = document.createElement("span");
      word.textContent = ` /${"long-folder-name-with-no-break/".repeat(40)}profile.toml`;
      says.append(word);
      const box = line.getBoundingClientRect();
      const measured = {
        right: box.right,
        window: window.innerWidth,
        overflows: line.scrollWidth > line.clientWidth,
        taller: box.height > short,
        page: document.documentElement.scrollWidth > window.innerWidth,
      };
      word.remove();
      return measured;
    }, cause);
    expect(wrapped).toBeDefined();
    expect(wrapped?.right).toBeLessThanOrEqual(wrapped?.window ?? 0);
    expect(wrapped?.overflows).toBe(false);
    expect(wrapped?.page).toBe(false);
    expect(wrapped?.taller).toBe(true);
    // Never written away: the store holds it, in its place, and names it gone.
    const held = await pins();
    expect(held.order).toEqual(before);
    expect(held.missing).toEqual([gone]);
    expect(await stripNames()).not.toContain(gone);

    renameSync(away, join(mine, "workspaces", gone));

    await browser.waitUntil(
      async () => {
        saw = await stripNames();
        return JSON.stringify(saw.slice(0, before.length)) === JSON.stringify(before);
      },
      { timeout: 30_000, timeoutMsg: `the pin came back as ${JSON.stringify(saw)}` },
    );
    await browser.waitUntil(async () => (await bandNotice(cause)).state === "absent", {
      timeout: 20_000,
      timeoutMsg: `the Notice that ${gone} is gone stayed`,
    });
  });
});
