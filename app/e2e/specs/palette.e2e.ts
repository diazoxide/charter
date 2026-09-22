import { browser, expect, $, $$ } from "@wdio/globals";
import { endChat, pickAndStart, pressAndStart, pressOnly } from "../opening.js";

/**
 * The command palette against the real app, driven by the keyboard and nothing else.
 *
 * **Not one click in this file reaches the palette.** `F2` opens it, the driver types into
 * the box it focused itself, Enter runs the aimed row and Escape leaves — which is the whole
 * claim spec decision 1 makes about it being the primary input.
 *
 * Two things here ARE clicked, and neither is the palette: the bar's own button, in the test
 * that runs the same action both ways and compares what the window became; and the profile
 * picker a row opens, for the WKWebView reason `answerThePicker` sets out.
 *
 * The characters are sent with `addValue` on the box rather than `browser.keys`, for the
 * reason `panes.e2e.ts` gives about typing into a terminal: keys aimed at the window go
 * through the driver's own mapping, which delivers some characters twice. `addValue` is the
 * WebDriver "element send keys" command — real keystrokes into the element that already has
 * the focus, and still no pointer anywhere near it.
 *
 * **It leaves the window as it found it.** That is not tidiness: specs run in name order and
 * share one app process, and `panes.e2e.ts` — which runs after this one — reaches for "the
 * first tab in the strip" and means its own. A tab left here becomes that one, and a spec
 * that has nothing to do with the palette fails on a chat it never opened.
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

/** The names on the tab strip, left to right. Scoped to that tablist: the strip above lists
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

/**
 * Answers the profile picker the palette opened.
 *
 * **`pickAndStart`, the shared helper, and not the keyboard** — which is a statement about
 * WKWebView rather than about either surface. macOS ships "press Tab to highlight each item
 * on a webpage" OFF, so in the webview this app runs in Tab moves between form fields and
 * skips buttons entirely: Cancel has the focus (the picker puts it there on purpose, so a
 * stray Return never launches anything) and Tab does not reach Start from it. A first run
 * proved that the hard way, and the picker is M1.2c's surface, not this milestone's.
 *
 * What this file claims, and tests, is narrower and true: **the palette needs no pointer.**
 * Every row is reached and run here by keystroke alone.
 */
async function answerThePicker() {
  await pickAndStart();
}

describe("the command palette", () => {
  /** The tabs that were already there, so only this file's own are closed again. */
  let wereAlreadyOpen: string[] = [];

  before(async () => {
    wereAlreadyOpen = await tabNames();
  });

  // **Every chat this file opened is closed again.** `panes.e2e.ts` runs after it and takes
  // the first tab in the strip to be its own; a leftover here becomes that tab, and its
  // assertion fails on a session it never typed into. A first run proved it.
  after(async () => {
    for (const name of (await tabNames()).filter((tab) => !wereAlreadyOpen.includes(tab))) {
      await endChat(`End chat ${name}`);
    }
    await browser.waitUntil(
      async () => (await tabNames()).every((tab) => wereAlreadyOpen.includes(tab)),
      { timeout: 20_000, timeoutMsg: "the palette spec left a chat open behind it" },
    );
  });

  // **Nothing below this file depends on it having run, and nothing it opens outlives it.**
  // `lifecycle.e2e.ts` states the same rule; this file learned it. One failed assertion left
  // the profile picker on screen, and because WebdriverIO's Tauri service keeps ONE app
  // process for the whole run, every spec after it failed on a modal it never opened.
  afterEach(async () => {
    for (const selector of [PALETTE, '[role="dialog"][aria-labelledby="start-chat"]']) {
      const dialog = await $(selector);
      if (await dialog.isDisplayed().catch(() => false)) {
        await browser.keys(["Escape"]);
        await dialog.waitForDisplayed({ reverse: true, timeout: 10_000 }).catch(() => undefined);
      }
    }
  });

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
    await answerThePicker();
    await expect($('[data-testid="pane"]')).toBeDisplayed();
  });

  it("closes the chat it just opened, by the keyboard alone", async () => {
    // The pane the test above opened is that tab's only one, so closing it closes the tab
    // with it — which is what makes this checkable without counting panes across tabs.
    const before = await tabNames();
    await openPalette();
    await typeIntoPalette("end this pane");

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
    await answerThePicker();
    const byPalette = await arrangement();

    expect(byPalette).toEqual(byButton);
  });

  it("hands F2 back to the chat when F2 is pressed again, and says so while it is up", async () => {
    // charter-app#47. The palette takes F2 on the window, capture-phase, so a harness that
    // binds F2 never sees it. tmux answers this with `send-prefix` and so does this: the
    // second press closes the palette and the key goes to the chat in front. What is tested
    // here is what an operator can see — the palette said there was a way out, and pressing
    // the key again took it.
    await pressAndStart("New tab");
    await openPalette();
    await expect($(".palette-through")).toHaveText(
      "Press F2 again to send F2 to the chat in front.",
    );

    await browser.keys(["F2"]);

    await expect($(PALETTE)).not.toBeDisplayed();
    // And it is the catalogue's own row that ran, which is what keeps the chord from being a
    // second implementation of it: the row is there, browsable, under the key's own name.
    await openPalette();
    await typeIntoPalette("F2");
    expect(await rows()).toEqual(["Send F2 to the chat in front"]);
    await browser.keys(["Escape"]);
  });
});
