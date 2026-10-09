import { realpathSync, renameSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { anEmptyRecord, copyFixturePlane } from "../harness.js";
import { closeProject } from "../opening.js";

/**
 * **A tab's menu opens Settings at that tab's level**, in the built app (#1213): the menu key on
 * the project's tab, then Project settings…, opens Settings at the Project level; on a
 * workspace's tab, then Workspace settings…, at that workspace's level. Both from the keyboard,
 * which SE-23 fixed on both strips.
 *
 * `src/settings/TabMenus.test.tsx` holds the rest of the ticket — the project right-clicked is
 * the one brought forward, and the palette's rows and the gears land on the tab the menu opened —
 * and `SettingsGears.test.tsx` that the menu key opens each tab's menu at all. What only a
 * scenario can say is that the path holds in the real window with the real core behind it.
 *
 * **The menu key is sent as the keydown the page would get.** WebDriver's key table has no
 * context-menu key, and a WebDriver keystroke is synthesised inside the page anyway (see
 * `settings-gears.e2e.ts`), so the spec dispatches the `keydown` on the tab that has the focus —
 * exactly what `Menus.openFromTheKeyboard` listens for (D-1213-b).
 *
 * **Its own project, copied into the run's tree**, opened through the opener and closed by its
 * tab's own × — one app process serves the whole run, so a project or a Settings tab left on a
 * strip here is what every later spec reads. Each Settings tab it opens is closed by its own ×
 * first.
 */

const PROJECTS = '[data-strip="Projects"]';
const WORKSPACES = '[data-strip="Workspaces"]';
const TABS = '[data-strip="Tabs"]';
const LEVEL = '[role="radiogroup"][aria-label="Level"]';

const mine = (() => {
  const copied = copyFixturePlane();
  const renamed = join(dirname(copied), "menus");
  renameSync(copied, renamed);
  const root = realpathSync(renamed);
  // Nothing to put back, so opening it starts no chat for the next spec to inherit.
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

/** The level the Settings tab on screen is at, by its switcher. */
async function levelBecomes(want: string): Promise<void> {
  let saw = "";
  await browser.waitUntil(
    async () => {
      saw = await browser.execute(
        (selector: string) =>
          document
            .querySelector(`${selector} [role="radio"][aria-checked="true"]`)
            ?.textContent?.trim() ?? "",
        LEVEL,
      );
      return saw === want;
    },
    { timeout: 30_000, timeoutMsg: `Settings stayed at ${JSON.stringify(saw)}, not ${want}` },
  );
}

/**
 * Gives `tab` the keyboard and presses the menu key on it, then presses the row called `row` in
 * the menu that opened.
 */
async function fromItsMenu(tab: string, row: string): Promise<void> {
  await $(tab).waitForExist({ timeout: 20_000 });
  await browser.execute((selector: string) => {
    const it = document.querySelector<HTMLElement>(selector);
    if (it === null) throw new Error(`no ${selector}`);
    it.focus();
    it.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ContextMenu", bubbles: true, cancelable: true }),
    );
  }, tab);
  // A row's text is its words and then its note, so it is found by how it starts.
  const item = await $(`//*[@role="menuitem"][starts-with(normalize-space(), "${row}")]`);
  await item.waitForDisplayed({ timeout: 20_000 });
  await item.click();
  await expect($('[role="menu"]')).not.toBeExisting();
}

/** The tab in front of the project in front. */
const inFront = () => $(`${TABS} [role="tab"][aria-selected="true"]`);

describe("a tab's menu opens Settings at that tab's level", function () {
  this.timeout(180_000);

  /** The project the launch opened, which this spec never touches. */
  let first = "";

  before(async () => {
    first = (await ask<string[]>("open_planes"))[0];
    // Through the opener, as a person opens one (`workspace-lifecycle.e2e.ts` says why).
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
    await $(`${WORKSPACES} [role="tab"].plane-root`).waitForExist({ timeout: 30_000 });
  });

  after(async () => {
    // The window as it was found: this spec's project closed from its own tab, which takes
    // anything left on its strips with it and brings the launch's project back to the front.
    const closer = `${PROJECTS} button[aria-label="Close project ${basename(mine)}"]`;
    if (await $(closer).isExisting()) {
      await closeProject(closer);
      await browser.waitUntil(async () => !(await $(closer).isExisting()), {
        timeout: 20_000,
        timeoutMsg: "this spec's project stayed on the strip",
      });
    }
    for (const plane of (await ask<string[]>("open_planes")).filter((one) => one !== first)) {
      await ask("close_plane", { plane });
    }
  });

  it("opens the Project level from the project's menu, and the workspace's from the workspace's", async () => {
    // A workspace focused, so the project's menu is held to the Project level and not to the
    // narrower one ⌘, would pick.
    const workspace = `${WORKSPACES} [role="tab"]:not(.plane-root)`;
    await $(workspace).click();
    const focused = await browser.execute(
      (selector: string) =>
        document.querySelector(`${selector} [role="tab"][aria-selected="true"] .workspace-name`)
          ?.textContent ?? "",
      WORKSPACES,
    );
    expect(focused).not.toBe("");

    await fromItsMenu(`${PROJECTS} [role="tab"][aria-selected="true"]`, "Project settings…");

    await levelBecomes("Project");
    await expect(inFront()).toHaveText("Settings", { containing: true });

    await fromItsMenu(`${WORKSPACES} [role="tab"][aria-selected="true"]`, "Workspace settings…");

    await levelBecomes("Workspace");
    const title = `Workspace settings · ${focused}`;
    await expect(inFront()).toHaveText(title, { containing: true });

    // Both tabs are on the focused workspace's strip; each is closed by its own ×.
    for (const name of [title, "Settings"]) {
      const closer = `${TABS} button[aria-label="Close ${name}"]`;
      await browser.execute((selector: string) => {
        document.querySelector<HTMLElement>(selector)?.click();
      }, closer);
      await browser.waitUntil(async () => !(await $(closer).isExisting()), {
        timeout: 20_000,
        timeoutMsg: `the ${name} tab stayed open`,
      });
    }
  });
});
