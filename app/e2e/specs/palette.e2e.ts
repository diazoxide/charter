import { browser, expect, $, $$ } from "@wdio/globals";
import { pickAndStart, pressAndStart, pressOnly } from "../opening.js";

/**
 * The command palette against the real app, driven by the keyboard and nothing else.
 *
 * **Not one click in this file reaches the palette.** `F2` opens it, the driver types into
 * the box it focused itself, Enter runs the aimed row and Escape leaves — which is the whole
 * claim spec decision 1 makes about it being the primary input. The one mouse press here is
 * the OTHER route: the bar's own button, in the test that runs the same action both ways and
 * compares what the window became.
 *
 * The characters are sent with `addValue` on the box rather than `browser.keys`, for the
 * reason `panes.e2e.ts` gives about typing into a terminal: keys aimed at the window go
 * through the driver's own mapping, which delivers some characters twice. `addValue` is the
 * WebDriver "element send keys" command — real keystrokes into the element that already has
 * the focus, and still no pointer anywhere near it.
 *
 * This spec runs before `panes.e2e.ts` (specs run in name order), which opens fifty
 * sessions, and it leaves behind only the tabs it opened.
 */

const PALETTE = '[role="dialog"][aria-label="Command palette"]';

async function openPalette() {
  await browser.keys(["F2"]);
  const up = await $(PALETTE);
  await up.waitForDisplayed({ timeout: 20_000 });
  return up;
}

/** Types into the box the palette focused itself. Nothing clicks it. */
async function typeIntoPalette(what: string) {
  const box = await $("#palette-query");
  await browser.waitUntil(async () => box.isFocused(), {
    timeout: 10_000,
    timeoutMsg: "the palette did not take the keyboard when it opened",
  });
  await box.addValue(what);
}

/** The rows on screen, top to bottom, as the operator reads them. */
async function rows(): Promise<string[]> {
  return browser.execute(() =>
    [...document.querySelectorAll('[role="option"]')].map(
      (row) => row.querySelector(".palette-title")?.textContent ?? "",
    ),
  );
}

/** The panes of the tab in front, and which of them has the keyboard. */
async function arrangement(): Promise<string[]> {
  return browser.execute(() =>
    [...document.querySelectorAll('[data-testid="pane"]')].map((pane) => pane.className),
  );
}

/** The names on the tab strip, left to right. Scoped to that tablist: the sidebar lists
 *  workspaces as a tablist too, and a query across the window would mix the two. */
async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [
      ...(document
        .querySelector('[role="tablist"][aria-label="Tabs"]')
        ?.querySelectorAll('[role="tab"]') ?? []),
    ].map((tab) => tab.querySelector(".tab-name")?.textContent ?? ""),
  );
}

/** Answers the profile picker without a pointer: Cancel has the focus, Start is next. */
async function pickAndStartByKeyboard() {
  const dialog = await $('[role="dialog"][aria-labelledby="start-chat"]');
  await dialog.waitForDisplayed({ timeout: 20_000 });
  // Cancel is focused first and on purpose: starting a chat runs a command with nothing
  // between the key and the exec, so it is never what a stray Return finds.
  await expect($("button=Cancel")).toBeFocused();
  await browser.keys(["Tab"]);
  await browser.keys(["Enter"]);
  await dialog.waitForDisplayed({ reverse: true, timeout: 30_000 });
}

describe("the command palette", () => {
  it("opens on a keystroke, wherever the keyboard happened to be", async () => {
    const up = await openPalette();

    await expect(up).toBeDisplayed();
    // Every action, not a chosen few: the bar shows four of these rows as buttons and the
    // palette shows all of them.
    expect(await rows()).toEqual(
      expect.arrayContaining(["New tab", "Split right", "Split down", "Quit charter"]),
    );
    await browser.keys(["Escape"]);
  });

  it("narrows as it is typed", async () => {
    await openPalette();

    await typeIntoPalette("split");

    expect(await rows()).toEqual(["Split right", "Split down"]);
    await browser.keys(["Escape"]);
  });

  it("lists an action that cannot run WITH its reason, rather than dropping it", async () => {
    // Nothing reports a hook in this run, so the needs-you queue is empty — and the row for
    // it is still here, saying so. An operator cannot ask about an option they cannot see.
    await openPalette();

    await typeIntoPalette("needs you");

    const row = await $('[role="option"]');
    await expect(row).toHaveAttribute("aria-disabled", "true");
    // The reason is words on the row, never the dimming alone.
    await expect(row).toHaveText(expect.stringContaining("Nothing needs you."));
    await browser.keys(["Escape"]);
  });

  it("leaves on Escape, having run nothing", async () => {
    const before = (await $$('[data-testid="pane"]').getElements()).length;
    await openPalette();
    await typeIntoPalette("new tab");

    await browser.keys(["Escape"]);

    await expect($(PALETTE)).not.toBeDisplayed();
    expect((await $$('[data-testid="pane"]').getElements()).length).toBe(before);
  });

  it("starts a chat, by the keyboard alone, and through the picker like every other route", async () => {
    await openPalette();
    await typeIntoPalette("new tab");

    await browser.keys(["Enter"]);

    // The palette got out of the way, and the question ADR 0022 insists on is on screen.
    await expect($(PALETTE)).not.toBeDisplayed();
    await pickAndStartByKeyboard();
    await expect($('[data-testid="pane"]')).toBeDisplayed();
  });

  it("closes the chat it just opened, by the keyboard alone", async () => {
    // The pane the test above opened is that tab's only one, so closing it closes the tab
    // with it — which is what makes this checkable without counting panes across tabs.
    const before = await tabNames();
    await openPalette();
    await typeIntoPalette("close pane");

    await browser.keys(["Enter"]);

    await browser.waitUntil(async () => (await tabNames()).length === before.length - 1, {
      timeout: 15_000,
      timeoutMsg: `the chat the palette was asked to close is still open: ${before.join(", ")}`,
    });
  });

  it("ends in the same arrangement whether a split came from the palette or from its button", async () => {
    // The test that would catch a second implementation hiding behind the same words.
    await pressAndStart("New tab");
    await pressOnly("Split right");
    await pickAndStart();
    const byButton = await arrangement();
    expect(byButton).toHaveLength(2);

    await pressAndStart("New tab");
    await openPalette();
    await typeIntoPalette("split right");
    await browser.keys(["Enter"]);
    await pickAndStartByKeyboard();
    const byPalette = await arrangement();

    expect(byPalette).toEqual(byButton);
  });
});
