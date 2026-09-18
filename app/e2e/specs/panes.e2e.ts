import { execFileSync } from "node:child_process";
import { browser, expect, $, $$ } from "@wdio/globals";
import { READY } from "../harness.js";

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
 *  Scoped to the tab strip by name: the sidebar lists workspaces as tabs too, so a
 *  document-wide `[role="tab"]` would mix a workspace in among them. */
async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [...document.querySelectorAll('[role="tablist"][aria-label="Tabs"] [role="tab"]')].map(
      (tab) => tab.textContent ?? "",
    ),
  );
}

/** How many fake harnesses this machine is running, asked of the operating system. */
function harnessesRunning(): number {
  const ps = execFileSync("ps", ["-A", "-o", "command="], { encoding: "utf8" });
  // The program a line RUNS, not any line that mentions the name, and one line is not one
  // process: a command line containing newlines is several lines of `ps` output. Both
  // mattered — a Claude Code session whose prompt discussed `fake-harness` was counted three
  // times, and this test failed at 53 of an expected 50 with nothing wrong.
  return ps.split("\n").filter((line) => /^\S*\/fake-harness(\s|$)/.test(line)).length;
}

describe("the window", () => {
  it("opens a session in a new tab, and the pane shows what it wrote", async () => {
    await press("New tab");

    const [pane] = await panes();
    await until(pane, READY);
  });

  it("sends what is typed to that session, and shows what it answers", async () => {
    const [pane] = await panes();

    await type(pane, "from the first pane");

    await until(pane, "you said: from the first pane");
  });

  it("splits the pane in front, and each half has a session of its own", async () => {
    await press("Split right");

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

    await press("New tab");
    // Chained, not one selector: WebdriverIO's `=text` shorthand is a whole selector and
    // cannot follow a CSS descendant part.
    await $('[role="tablist"][aria-label="Tabs"]').$(`[role="tab"]=${first}`).click();

    const [pane] = await panes();
    await until(pane, "you said: from the first pane");
  });

  it("keeps fifty sessions running at once, and stays usable", async () => {
    for (let opened = 0; harnessesRunning() < 50 && opened < 60; opened++) {
      await press("New tab");
    }

    expect(harnessesRunning()).toBe(50);
    const [pane] = await panes();
    await until(pane, READY);
    await type(pane, "with fifty sessions running");
    await until(pane, "you said: with fifty sessions running");
  });

  it("ends a session when its tab closes", async () => {
    const running = harnessesRunning();
    const names = await tabNames();
    const closing = names[names.length - 1];

    await press(`Close tab ${closing}`);

    await browser.waitUntil(async () => harnessesRunning() === running - 1, {
      timeout: 15_000,
      timeoutMsg: `closing tab ${closing} left its session running`,
    });
  });
});
