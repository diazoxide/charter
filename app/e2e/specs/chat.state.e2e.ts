import { browser, expect, $, $$ } from "@wdio/globals";
import { pressAndStart } from "../opening.js";
import { READY } from "../harness.js";

/**
 * A harness's own hook, through the real binary and the real socket, to what the window draws.
 *
 * This is the whole of spec decision 3 in one test: nothing here reads the session's output to
 * decide anything (the pane's text is only used to know when the harness got to the point of
 * firing its hook). The state on screen came from `charter hook`, over the socket the app
 * opened, into the board — and the window was told rather than asked.
 */

/** What a tab says its chat is doing, by the accessible name on its state mark. */
async function doing(tab: WebdriverIO.Element): Promise<string> {
  const mark = await tab.$(".state");
  return (await mark.getAttribute("aria-label")) ?? "";
}

/** The one tab on screen, once there is one. */
async function theTab(): Promise<WebdriverIO.Element> {
  await browser.waitUntil(
    async () =>
      (await $$('[role="tablist"][aria-label="Tabs"] [role="tab"]').getElements()).length >= 1,
    { timeout: 20_000, timeoutMsg: "no tab ever appeared" },
  );
  return await $('[role="tablist"][aria-label="Tabs"] [role="tab"]').getElement();
}

/** Waits until the tab says `state`, and says what it said instead if it never does. */
async function until(state: string): Promise<void> {
  let last = "";
  try {
    await browser.waitUntil(
      async () => {
        last = await doing(await theTab());
        return last === state;
      },
      { timeout: 30_000, interval: 200 },
    );
  } catch {
    throw new Error(
      `the chat never showed ${JSON.stringify(state)}; it showed ${JSON.stringify(last)}`,
    );
  }
}

describe("what a chat is doing", () => {
  before(async () => {
    // The app opens no chat by itself; a person presses this, and so does every other
    // scenario spec.
    await pressAndStart("New tab");
  });

  it("shows a turn running, because the harness's hook said so", async () => {
    // The shell the app opens runs `charter hook userpromptsubmit` before it writes a byte,
    // and then holds its output. Nothing about the app knows this is a test, and nothing
    // about this state came from anything the session printed.
    await until("running");
  });

  it("shows the chat waiting on you once the harness's turn ends, and queues it", async () => {
    // Nothing needs the operator while the turn is running.
    await expect($('[aria-label="Needs you"]')).toHaveText(
      expect.stringContaining("Nothing needs you"),
    );

    // Releasing the output is what lets the harness get to its `stop` hook.
    //
    // Through the terminal's own hidden input, never `browser.keys`: the embedded driver's
    // key mapping delivers every character twice and turns some letters into function keys
    // ("from the first pane" arrives as "ffRroommthheeffiiRrSstPpaannee"). Element Send Keys
    // on `.xterm-helper-textarea` is one path with no duplicates.
    const pane = await $('[data-testid="pane"]');
    await pane.click();
    await $(".xterm-helper-textarea").addValue("\n");
    await browser.waitUntil(
      async () => (await (await pane.$(".xterm-rows")).getText()).includes(READY),
      { timeout: 30_000, interval: 250, timeoutMsg: "the harness never finished its output" },
    );

    await until("waiting on you");

    const queue = await $('[aria-label="Needs you"]');
    await expect(queue).toHaveText(expect.stringContaining("1 need you"));
  });
});
