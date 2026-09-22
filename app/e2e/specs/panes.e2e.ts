import { endChat, pressAndStart } from "../opening.js";
import { browser, expect, $, $$ } from "@wdio/globals";
import { READY } from "../harness.js";
import { harnessesRunning } from "../processes.js";

/**
 * The skeleton, driven the way a person drives it: buttons, tabs and typing, against the real
 * app with the fake harness in every pane.
 */

/** One pane, as a found element. */
type Pane = WebdriverIO.Element;

/** Everything a pane's terminal is showing, as one string. */
async function showing(pane: Pane): Promise<string> {
  const rows = await pane.$(".xterm-rows");
  return (await rows.getText()).replace(/\s+/g, " ");
}

/** The panes on screen, once there are `many` of them. */
async function panes(many = 1): Promise<Pane[]> {
  const found = async () => [...(await $$('[data-testid="pane"]').getElements())];
  await browser.waitUntil(async () => (await found()).length >= many, {
    timeout: 10_000,
    timeoutMsg: `there were never ${many} panes on screen`,
  });
  return found();
}

/** Waits until a pane shows `text`, and says what it showed instead if it never does. */
async function until(pane: Pane, text: string): Promise<void> {
  let last = "";
  try {
    await browser.waitUntil(
      async () => {
        last = await showing(pane);
        return last.includes(text);
      },
      { timeout: 30_000, interval: 250 },
    );
  } catch {
    throw new Error(`the pane never showed ${JSON.stringify(text)}; it showed ${last}`);
  }
}

/**
 * Types into a pane: click it, so it takes the keyboard, then send the text to the terminal's
 * own input, which is where a terminal reads what is typed.
 *
 * Sending the keys to the window instead goes through the driver's own key mapping, which
 * delivers each character twice and turns some of them into function keys.
 */
async function type(pane: Pane, text: string): Promise<void> {
  await pane.click();
  await pane.$(".xterm-helper-textarea").addValue(`${text}\n`);
}

/** What the tab bar shows, left to right. Each tab is named by its number.
 *
 *  Scoped to the tab strip by name: the workspaces are a tablist too, so a
 *  document-wide `[role="tab"]` would mix a workspace in among them. */
async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [...document.querySelectorAll('[role="tablist"][aria-label="Tabs"] [role="tab"]')].map(
      (tab) => tab.textContent ?? "",
    ),
  );
}

/**
 * How many tabs charter itself says are open, counted from the palette.
 *
 * **The one answer to "how many tabs are there" that does not come from the strip under
 * test.** The catalogue emits one `End chat <name>` row per tab and the palette lists them
 * all, which is what makes it the right oracle for a strip that no longer draws every tab —
 * and it is the find surface charter ADR 0039 names, so it is not a second one invented here.
 *
 * **Tabs and not sessions**, which is the distinction that made the first version of the
 * assertion below wrong on both platforms: a split puts two sessions in ONE tab, the spec
 * three above this one leaves exactly such a tab behind, and `harnessesRunning()` therefore
 * counted one more than the strip could ever have shown.
 */
async function tabsOpen(): Promise<number> {
  await browser.keys(["F2"]);
  const palette = await $('[role="dialog"][aria-label="Command palette"]');
  await palette.waitForDisplayed({ timeout: 20_000 });
  const counted = await browser.execute(
    () =>
      [...document.querySelectorAll('[role="option"] .palette-title')].filter((row) =>
        (row.textContent ?? "").startsWith("End chat "),
      ).length,
  );
  await browser.keys(["Escape"]);
  await expect(palette).not.toBeDisplayed();
  return counted;
}

describe("the window", () => {
  it("opens a session in a new tab, and the pane shows what it wrote", async () => {
    await pressAndStart("New tab");

    const [pane] = await panes();
    await until(pane, READY);
  });

  it("sends what is typed to that session, and shows what it answers", async () => {
    const [pane] = await panes();

    await type(pane, "from the first pane");

    await until(pane, "you said: from the first pane");
  });

  it("splits the pane in front, and each half has a session of its own", async () => {
    await pressAndStart("Split right");

    const [first, second] = await panes(2);
    await until(second, READY);
    // The new pane is the focused one, so what is typed goes there and not to its neighbour.
    await type(second, "only in the new pane");
    await until(second, "you said: only in the new pane");
    expect(await showing(first)).not.toContain("only in the new pane");
  });

  it("draws a session's screen again when its tab comes back to the front", async () => {
    // The pane that was showing it is gone, and with it its terminal: what comes back can
    // only come from the screen the core kept.
    const [first] = await tabNames();

    await pressAndStart("New tab");
    // Chained, not one selector: WebdriverIO's `=text` shorthand is a whole selector and
    // cannot follow a CSS descendant part.
    await $('[role="tablist"][aria-label="Tabs"]').$(`[role="tab"]=${first}`).click();

    const [pane] = await panes();
    await until(pane, "you said: from the first pane");
  });

  it("keeps fifty sessions running at once, and stays usable", async () => {
    for (let opened = 0; harnessesRunning() < 50 && opened < 60; opened++) {
      await pressAndStart("New tab");
    }

    expect(harnessesRunning()).toBe(50);
    const [pane] = await panes();
    await until(pane, READY);
    await type(pane, "with fifty sessions running");
    await until(pane, "you said: with fifty sessions running");
  });

  /**
   * The strip collapsing, against a real layout (charter ADR 0039, as amended).
   *
   * **This is the only place the measurement itself is under test.** How many tabs fit is a
   * property of the strip's laid-out width, and jsdom gives every element a zero-sized box
   * (#149) — so the unit tests hand `fitting` a width and this is what says a real WebView
   * ever produces one. It runs straight after the fifty-session test, which is the state that
   * makes the question real.
   *
   * It is also where the operator's own complaint is held: *"i noticed that tabs now
   * scrollable — instead of automatic expanding in show more button."* A scroller coming back
   * would pass every unit test in the repo.
   *
   * **This spec can now assert what tabs are drawn, which the version before it could not.**
   * The old measurement was an `IntersectionObserver`, answered in the engine's rendering
   * step, and macOS gives a WKWebView no rendering while its window is covered — Linux
   * reported 3 of 51 tabs visible against macOS's 0 of 49, same commit (charter-app M0.6).
   * What replaced it reads `clientWidth` in a layout effect, and a layout is not a paint.
   */
  it("shows what fits, hides the rest behind one button, and does not scroll", async () => {
    const drawn = await tabNames();
    const open = await tabsOpen();
    expect(open).toBeGreaterThan(40);

    // On the bar, and by what it SAYS rather than by its class: the accessible name is the
    // whole of what an operator gets from it before they open it.
    const more = await $('.bar button[aria-label^="Show "]');
    await more.waitForExist({
      timeout: 10_000,
      timeoutMsg:
        `fifty chats did not overflow the strip: it drew ${drawn.length} tabs and nothing ` +
        `said there were more. A strip that has never measured its own width draws ` +
        `everything, so this is either a layout that never happened or a collapse that is ` +
        `not working at all`,
    });
    const said = await more.getAttribute("aria-label");
    const counted = Number(/^Show (\d+) tabs? /.exec(said ?? "")?.[1]);

    // **Every tab is in exactly one of the two places.** This is the whole of what the
    // collapse owes and the assertion the scroller could not make: nothing is both drawn and
    // hidden, and nothing is neither. `open` is charter's own count and not the strip's —
    // see `tabsOpen`, and why it is tabs rather than sessions.
    expect(drawn.length + counted).toBe(open);
    expect(drawn.length).toBeGreaterThan(0);
    expect(counted).toBeGreaterThan(0);

    // **And the strip really does not scroll.** `overflow: hidden` means a scrollbar cannot
    // appear, so what this catches is the other half: tabs laid out wider than the strip they
    // sit in, which would be tabs clipped and unreachable rather than collapsed and listed.
    const over = await browser.execute(() => {
      const strip = document.querySelector('[role="tablist"][aria-label="Tabs"]');
      return strip === null ? -1 : strip.scrollWidth - strip.clientWidth;
    });
    expect(over).toBeLessThanOrEqual(1);

    // **No tab is drawn narrower than the floor its strip fits by**, on any of the three.
    //
    // This is the invariant the arithmetic rests on and the one thing about it that only a
    // real layout can check: `fits.ts` answers "how many fit" as `width / least`, which is a
    // lie the moment a stylesheet rule lets a tab shrink past `least`. It is not hypothetical
    // — a `min-width: 0` meant for the button inside a tab also matched the workspace strip's
    // tabs, which ARE their own cells, and won on source order. The strip then squeezed eight
    // workspaces into the room its own arithmetic had given four.
    //
    // The floor is read off the element, so this and the app cannot disagree about the number.
    const squeezed = await browser.execute(() => {
      const strips = ["Projects", "Workspaces", "Tabs"];
      const narrow: string[] = [];
      for (const named of strips) {
        const strip = document.querySelector(`[role="tablist"][aria-label="${named}"]`);
        if (!strip) continue;
        const least = Number.parseFloat(getComputedStyle(strip).getPropertyValue("--least"));
        if (!Number.isFinite(least)) {
          narrow.push(`${named}: no --least on the strip`);
          continue;
        }
        for (const tab of strip.querySelectorAll('[role="tab"]')) {
          // The CELL, which is what carries the floor: a wrapper where there is one (a tab and
          // its `×`), and the button itself where there is not.
          const cell = tab.closest(".project, .tab") ?? tab;
          const wide = cell.getBoundingClientRect().width;
          if (wide < least - 1) narrow.push(`${named}: a tab is ${wide}px, under ${least}px`);
        }
      }
      return narrow;
    });
    expect(squeezed).toEqual([]);

    await more.click();
    // **Waited for, not read once.** The menu is a Radix portal: it is mounted on the open,
    // in a later frame than the click, so a `$$` taken straight after the click finds an
    // empty document and reports it as "the menu listed nothing". Measured on this spec's
    // first CI run, which is how this comment came to exist.
    const rowsNow = async () => [...(await $$('[role="menuitem"]').getElements())];
    let found = 0;
    try {
      await browser.waitUntil(
        async () => {
          found = (await rowsNow()).length;
          return found === counted;
        },
        { timeout: 10_000 },
      );
    } catch {
      // Which of the two things went wrong, said in the failure: a menu that never opened is
      // a different defect from a menu that opened listing the wrong tabs.
      const menus = (await $$('[role="menu"]').getElements()).length;
      throw new Error(
        `the button said ${counted} tabs; ${menus} menu(s) opened and ${found} rows were listed`,
      );
    }
    const rows = await rowsNow();

    // The row's own name, so the assertion is about the tab this went to and not about
    // whichever tab happens to be first.
    const going = (await rows[0].$(".tab-name").getText()).trim();
    await rows[0].click();

    await browser.waitUntil(
      async () =>
        (await browser.execute(
          () =>
            document
              .querySelector(
                '[role="tablist"][aria-label="Tabs"] [role="tab"][aria-selected="true"]',
              )
              ?.querySelector(".tab-name")?.textContent ?? "",
        )) === going,
      { timeout: 10_000, timeoutMsg: `the menu did not bring ${going} to the front` },
    );

    // **And it arrived WITH its close button**, which is what keeps ending a chat two presses
    // rather than one from a menu under the cursor (charter-app#130).
    await expect(
      $(`[role="tablist"][aria-label="Tabs"] button[aria-label="End chat ${going}"]`),
    ).toBeExisting();

    // **And the strip did not re-order.** The menu sorts by activity; the strip never does.
    // What it draws now is the tab that was brought forward plus some of what it drew before,
    // still in the order it drew them.
    const after = await tabNames();
    const kept = after.filter((name) => drawn.includes(name));
    expect(kept).toEqual(drawn.filter((name) => kept.includes(name)));
  });

  it("ends a session when its tab closes", async () => {
    const running = harnessesRunning();
    const names = await tabNames();
    const closing = names[names.length - 1];

    await endChat(`End chat ${closing}`);

    await browser.waitUntil(async () => harnessesRunning() === running - 1, {
      timeout: 15_000,
      timeoutMsg: `closing tab ${closing} left its session running`,
    });
  });
});
