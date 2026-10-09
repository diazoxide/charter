import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";

/**
 * **Settings › Forges adds and removes a `[[forge]]` block**, in the built app (ST-3, #1227).
 *
 * Opened the way the operator opens it — the palette's Project settings… — on the copy of the
 * fixture plane this run was given. A forge is added through the inline form, `charter.toml` on
 * disk is read to see the block the core wrote, and the same block is removed with its row's
 * Remove, leaving the file without it.
 *
 * What is proved here and not in jsdom: `add_project_forge` and `remove_project_forge` exist,
 * write a real file through `purlis_core::settings::forges`, and the window re-reads what they
 * wrote.
 *
 * **It leaves the window and the plane as it found them**: one app process serves the whole run
 * (a leaking spec broke train 38), so `charter.toml` is put back byte for byte whatever happened,
 * and a Settings tab this spec opened is closed.
 */

const TABS = '[data-strip="Tabs"]';
const PALETTE = '[role="dialog"][aria-label="Command palette"]';
const SETTINGS = "Settings";
const HOST = "git.st3-e2e.invalid";
const OWNER = "st3-e2e";

/** The plane the app says it is acting on — the copy this run was given, never the repo's. */
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
  await $('nav[aria-label="Groups"]').waitForDisplayed({ timeout: 20_000 });
}

describe("Settings › Forges", () => {
  let file = "";
  let original: string | null = null;
  let tabWasOpen = false;

  before(async () => {
    file = join(await planeRoot(), "charter.toml");
    original = existsSync(file) ? readFileSync(file, "utf8") : null;
    tabWasOpen = (await tabNames()).some((name) => name.includes(SETTINGS));
  });

  after(async () => {
    // Whatever happened, the file is what it was before this spec: put back, or taken away
    // again when there was none. A `before` that failed read nothing, so touches nothing.
    if (file !== "") {
      const now = existsSync(file) ? readFileSync(file, "utf8") : null;
      if (original === null) {
        if (now !== null) rmSync(file);
      } else if (now !== original) writeFileSync(file, original);
    }
    if (!tabWasOpen) {
      const closer = await $(`${TABS} button[aria-label="Close ${SETTINGS}"]`);
      if (await closer.isExisting()) await closer.click();
    }
  });

  it("adds a forge that lands in charter.toml, and removes it again", async () => {
    await openProjectSettings();
    await $('nav[aria-label="Groups"]').$("button=Forges").click();

    await $("button=Add forge").click();
    const form = await $('form[aria-label="New forge"]');
    await form.waitForDisplayed({ timeout: 10_000 });
    await form.$("label=Owner").click();
    await browser.keys(OWNER.split(""));
    await form.$("label=Host").click();
    await browser.keys(HOST.split(""));
    await form.$("button=Add forge").click();

    await browser.waitUntil(async () => readFileSync(file, "utf8").includes(`host = "${HOST}"`), {
      timeout: 20_000,
      timeoutMsg: `charter.toml never held the forge at ${HOST}`,
    });
    const written = readFileSync(file, "utf8");
    expect(written).toContain("[[forge]]");
    expect(written).toContain(`owner = "${OWNER}"`);

    const remove = await $(`button[aria-label$="gitlab ${OWNER} at ${HOST}"]`);
    await remove.waitForDisplayed({ timeout: 20_000 });
    await remove.click();

    await browser.waitUntil(async () => !readFileSync(file, "utf8").includes(HOST), {
      timeout: 20_000,
      timeoutMsg: `charter.toml still holds the forge at ${HOST}`,
    });
    expect(readFileSync(file, "utf8")).toBe(original);
  });
});
