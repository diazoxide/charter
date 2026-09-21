import { pressAndStart } from "../opening.js";
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

/** Presses the button a person would read as `name`: its label, or the text on it. */
async function press(name: string): Promise<void> {
  const labelled = await $(`button[aria-label="${name}"]`);
  if (await labelled.isExisting()) {
    await labelled.click();
  } else {
    await $(`button=${name}`).click();
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
   * The show-more menu, against a real layout (charter ADR 0039).
   *
   * **This is the only place the measurement itself is under test.** What does not fit is a
   * property of the strip's width, the tabs in it and where it is scrolled to, and jsdom
   * gives every element a zero-sized box — so the unit tests drive an observer of their own
   * and this is what says the real one answers anything at all. It runs straight after the
   * fifty-session test, which is the state that makes the question real.
   */
  it("says how many tabs it is not showing, and gets to one of them", async () => {
    const before = await tabNames();
    expect(before.length).toBeGreaterThan(40);

    // On the bar, and by what it SAYS rather than by its class: the accessible name is the
    // whole of what an operator gets from it before they open it.
    const more = await $('.bar button[aria-label^="Show "]');
    await more.waitForExist({
      timeout: 10_000,
      timeoutMsg: "fifty tabs did not overflow the strip, so nothing said there were more",
    });
    const said = await more.getAttribute("aria-label");
    const counted = Number(/^Show (\d+) tabs? /.exec(said ?? "")?.[1]);
    expect(counted).toBeGreaterThan(0);
    expect(counted).toBeLessThanOrEqual(before.length);
    // **And deliberately no assertion that some tabs ARE on the strip**, which is what this
    // line tried to say for two runs. An `IntersectionObserver` is answered in the browser's
    // own rendering step, and macOS gives a WKWebView no rendering at all while its window
    // is covered or the display is asleep — measured in charter-app M0.6 and again here:
    // Linux reported 3 of 51 tabs visible and macOS reported 0 of 49, same build, same
    // commit. On a runner that stops rendering, the first delivery is the only delivery and
    // it lands before the strip has its width. So what this spec holds is what does not
    // depend on the window still being drawn — that the measurement produced a count, that
    // the menu lists exactly that many, that a row reaches its tab, and that the strip did
    // not move. Which tabs are visible is not a question a covered window can be asked.

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
    // **And the strip did not move.** The menu sorts by activity; the strip never does.
    expect(await tabNames()).toEqual(before);
  });

  it("ends a session when its tab closes", async () => {
    const running = harnessesRunning();
    const names = await tabNames();
    const closing = names[names.length - 1];

    await press(`End chat ${closing}`);

    await browser.waitUntil(async () => harnessesRunning() === running - 1, {
      timeout: 15_000,
      timeoutMsg: `closing tab ${closing} left its session running`,
    });
  });
});
