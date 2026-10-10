import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";

/**
 * **Settings › Harness & profiles adds a profile, gives it a page, and removes it**, in the built
 * app (#1290's last line; ST-4, #1236).
 *
 * Opened the way the operator opens it, the palette's Project settings…, on the copy of the
 * fixture project this run was given. A profile is added through Harness & profiles' Add form,
 * `charter.local.toml` on disk is read to see the table the core wrote, the profile's own page is
 * opened from the nav, and its Remove takes the table out of the file again.
 *
 * What is proved here and not in jsdom: the profiles collection's commands exist, write the
 * project's real local settings file through `purlis_core::settings::harness_profiles`, and the
 * window reads back what they wrote, page and all. `ProfilesCollection.test.tsx` holds the rest.
 *
 * **It leaves the window and the project as it found them**: one app process serves the whole
 * run, so `charter.local.toml` is put back byte for byte whatever happened, and a Settings tab
 * this spec opened is closed.
 */

const TABS = '[data-strip="Tabs"]';
const PALETTE = '[role="dialog"][aria-label="Command palette"]';
const GROUPS = 'nav[aria-label="Groups"]';
const SETTINGS = "Settings";
const NAME = "e2e-profile";
const TABLE = `[harness.${NAME}]`;

/** The project the app says it is acting on: the copy this run was given, never the repo's. */
async function planeRoot(): Promise<string> {
  const said = await $("span.plane code");
  await said.waitForDisplayed({ timeout: 30_000 });
  return (await said.getText()).trim();
}

/** The names on the tab strip. */
async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [
      ...(document.querySelector('[data-strip="Tabs"]')?.querySelectorAll('[role="tab"]') ?? []),
    ].map((tab) => tab.querySelector(".tab-name")?.textContent ?? tab.textContent ?? ""),
  );
}

/** Opens Settings at the Project level through the palette, as the operator does. */
async function openProjectSettings(): Promise<void> {
  await browser.keys(["F2"]);
  await $(PALETTE).waitForDisplayed({ timeout: 20_000 });
  const box = await $("#palette-query");
  await browser.waitUntil(async () => box.isFocused(), {
    timeout: 10_000,
    timeoutMsg: "the palette did not take the keyboard when it opened",
  });
  await box.addValue("Project settings");
  await browser.waitUntil(
    async () =>
      browser.execute(
        () =>
          document
            .querySelector('[role="option"][aria-selected="true"] .palette-title')
            ?.textContent?.startsWith("Project settings") ?? false,
      ),
    { timeout: 10_000, timeoutMsg: "the palette did not offer Project settings…" },
  );
  await browser.keys(["Enter"]);
  await $(GROUPS).waitForDisplayed({ timeout: 20_000 });
}

/** The local settings file as it is on disk now, or `null` when there is none. */
const onDisk = (file: string): string | null =>
  existsSync(file) ? readFileSync(file, "utf8") : null;

describe("Settings › Harness & profiles", () => {
  let root = "";
  let file = "";
  let original: string | null = null;
  let tabWasOpen = false;

  before(async () => {
    root = await planeRoot();
    file = join(root, "charter.local.toml");
    original = onDisk(file);
    tabWasOpen = (await tabNames()).some((name) => name.includes(SETTINGS));
  });

  after(async () => {
    // Whatever happened, the file is what it was before this spec: put back, or taken away
    // again when there was none. A `before` that failed read nothing, so touches nothing.
    if (file !== "") {
      const now = onDisk(file);
      if (original === null) {
        if (now !== null) rmSync(file);
      } else if (now !== original) writeFileSync(file, original);
    }
    if (!tabWasOpen) {
      const closer = await $(`${TABS} button[aria-label="Close ${SETTINGS}"]`);
      if (await closer.isExisting()) await closer.click();
    }
  });

  it("adds a profile that lands in charter.local.toml with a page of its own, and removes it", async () => {
    expect(original ?? "").not.toContain(TABLE);
    await openProjectSettings();
    await $(GROUPS).$("button=Harness & profiles").click();

    await $("button=Add profile").click();
    const form = await $('form[aria-label="New profile"]');
    await form.waitForDisplayed({ timeout: 10_000 });
    await form.$("label=Name").click();
    await browser.keys(NAME.split(""));
    // The stand-in the run's own profiles start (`declareAProfile`): a program that is there.
    await form.$("label=Command").click();
    await browser.keys(join(root, "claude-stand-in").split(""));
    await form.$("button=Add profile").click();

    await browser.waitUntil(async () => (onDisk(file) ?? "").includes(TABLE), {
      timeout: 20_000,
      timeoutMsg: `charter.local.toml never held ${TABLE}`,
    });
    const written = onDisk(file) ?? "";
    expect(written).toContain('kind = "claude"');
    expect(written).toContain(join(root, "claude-stand-in"));

    // Its own page, from the nav, which is where its Remove is.
    const page = await $(GROUPS).$(`button=${NAME}`);
    await page.waitForDisplayed({
      timeout: 20_000,
      timeoutMsg: `the nav never listed ${NAME}'s page`,
    });
    await page.click();
    const remove = await $(`button[aria-label="Remove ${NAME}"]`);
    await remove.waitForDisplayed({ timeout: 20_000 });

    await remove.click();
    await browser.waitUntil(async () => !(onDisk(file) ?? "").includes(TABLE), {
      timeout: 20_000,
      timeoutMsg: `charter.local.toml still holds ${TABLE}`,
    });
    // The profiles the run started with are still there, and the page went with the profile.
    expect(onDisk(file) ?? "").toContain("[harness.scenario]");
    await $(GROUPS)
      .$(`button=${NAME}`)
      .waitForExist({
        timeout: 20_000,
        reverse: true,
        timeoutMsg: `the nav still lists ${NAME}'s page after it was removed`,
      });
  });
});
