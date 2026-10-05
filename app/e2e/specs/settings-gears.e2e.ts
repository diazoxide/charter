import { existsSync, readFileSync, realpathSync, renameSync } from "node:fs";
import { dirname, join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { anEmptyRecord, copyFixturePlane } from "../harness.js";

/**
 * **The quiet gears, and ⌘, at the focused level**, in the built app (SE-23, #1173; V89g and
 * V89i on #558): the gear on the project in front opens Settings at the Project level, a value
 * changed there lands in `charter.toml` on disk, and the app menu's Settings… opens the focused
 * workspace's level.
 *
 * `src/settings/SettingsGears.test.tsx` owns which tab carries a gear, its name, its Tab stop and
 * the stylesheet's quiet; what only a scenario can say is that the path holds with the real core
 * behind it and the real file under it.
 *
 * **⌘, is sent as the event the menu sends** (`settings-asked`, `lifecycle.rs`). The accelerator
 * is a native menu's, which a WebDriver keystroke never reaches — it is synthesised inside the
 * page. `lifecycle.rs`'s tests hold the accelerator to `⌘,` and `Ctrl+,`; this holds everything
 * after it.
 *
 * **Its own project, copied into the run's tree** (charter-app#129's fence): it writes a setting,
 * so it may only ever be pointed at a plane the run itself made, and it closes it at the end.
 */

const PROJECTS = '[role="tablist"][aria-label="Projects"]';
const WORKSPACES = '[role="tablist"][aria-label="Workspaces"]';
const LEVEL = '[role="radiogroup"][aria-label="Level"]';

const mine = (() => {
  const copied = copyFixturePlane();
  const renamed = join(dirname(copied), "gears");
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

const sharedFile = () => join(mine, "charter.toml");
const localFile = () => join(mine, "charter.local.toml");

describe("the settings gears, and ⌘, at the focused level", function () {
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
    for (const plane of (await ask<string[]>("open_planes")).filter((one) => one !== first)) {
      await ask("close_plane", { plane });
    }
  });

  it("opens Project settings from the gear on the project in front, and writes charter.toml", async () => {
    // The plane root is not a workspace, so nothing narrower than the project is focused.
    await $(`${WORKSPACES} [role="tab"].plane-root`).click();

    // Quiet until the pointer is on its tab: the pointer goes there first, as a person's does.
    const tab = await $(`${PROJECTS} [role="tab"][aria-selected="true"]`);
    await tab.moveTo();
    const gear = await $(`${PROJECTS} button.gear[aria-label^="Project settings"]`);
    await gear.waitForClickable({ timeout: 20_000 });
    await gear.click();

    await levelBecomes("Project");

    await $('nav[aria-label="Groups"]').$("button=Saving").click();
    // An on/off setting is a native `<select>` (on, off, not set), so it is chosen, not clicked.
    const sign = await $("aria/Sign commits");
    await sign.selectByAttribute("value", "on");

    // Shared by default, so it lands in the committed file and not in this machine's.
    await browser.waitUntil(
      async () => /sign\s*=\s*true/.test(readFileSync(sharedFile(), "utf8")),
      { timeout: 30_000, timeoutMsg: "charter.toml never said sign = true" },
    );
    expect(existsSync(localFile()) ? readFileSync(localFile(), "utf8") : "").not.toMatch(/sign/);
  });

  it("opens the focused workspace's level on the app menu's Settings…", async () => {
    await $(`${WORKSPACES} [role="tab"]:not(.plane-root)`).click();
    const focused = await browser.execute(
      (selector: string) =>
        document.querySelector(`${selector} [role="tab"][aria-selected="true"] .workspace-name`)
          ?.textContent ?? "",
      WORKSPACES,
    );
    expect(focused).not.toBe("");

    await browser.executeAsync((done: () => void) => {
      void window.__TAURI__.event.emit("settings-asked").then(done, done);
    });

    await levelBecomes("Workspace");
    await expect(
      $('[role="tablist"][aria-label="Tabs"] [role="tab"][aria-selected="true"]'),
    ).toHaveText(`Workspace settings · ${focused}`, { containing: true });
  });
});
