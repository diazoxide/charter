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

/**
 * The title bar's needs-you button (charter-app#249), by what it is to the operator: the
 * control in the bar that opens a menu and is named for chats needing you.
 */
const HAND = '[data-testid="title-bar"] [aria-haspopup="menu"]';

/** Waits until the hand's name says `want`, and says what it said instead if it never does. */
async function untilTheHandSays(want: RegExp): Promise<WebdriverIO.Element> {
  let last = "";
  try {
    await browser.waitUntil(
      async () => {
        const hand = await $(HAND);
        last = (await hand.isExisting()) ? ((await hand.getAttribute("aria-label")) ?? "") : "";
        return want.test(last);
      },
      { timeout: 20_000, interval: 200 },
    );
  } catch {
    throw new Error(`the title bar's hand never said ${want}; it said ${JSON.stringify(last)}`);
  }
  return await $(HAND).getElement();
}

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
    // Nothing has ASKED for the operator while the turn is running — and this run's profile
    // is declared `codex`, so this chat is exactly the one charter-app#52 is about: it cannot
    // say it has stopped mid-turn for an approval. So the title bar neither claims nothing
    // needs you nor shows a count: a faint hand with no number, named for the chat (the
    // operator's ruling on #249).
    const faint = await untilTheHandSays(
      /^Nothing has asked for you, but .+ can't tell charter it's waiting$/,
    );
    await expect(faint).toHaveElementClass("muted");
    await expect(faint).toHaveAttribute("tabindex", "0");
    await expect(faint).not.toHaveText(expect.stringMatching(/\d/));

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

    // Queued, in the title bar: a hand and a count (charter-app#249).
    const hand = await untilTheHandSays(/^1 chat needs you$/);
    await expect(hand).not.toHaveElementClass("muted");
    await expect(hand).toHaveText("1");
    await expect(hand).toHaveAttribute("tabindex", "0");
  });

  it("lists the chat in the title bar's dropdown, and Go brings it forward", async () => {
    const hand = await $(HAND);
    await hand.click();

    const item = await $('[role="menu"] [role="menuitem"]');
    await item.waitForDisplayed({ timeout: 10_000 });
    // Its name, then its workspace and its project, then Go.
    await expect(item).toHaveAttribute("aria-label", expect.stringMatching(/^Go to .+ · .+ · .+$/));
    await expect(item).toHaveText(expect.stringContaining("Go"));
    // And the Ignore beside it, named for what it does and out of the arrows' way.
    const ignore = await $('[role="menu"] [aria-label^="Ignore "]');
    await expect(ignore).toHaveAttribute(
      "aria-label",
      expect.stringMatching(/^Ignore .+ until it asks again$/),
    );
    await expect(ignore).toHaveAttribute("tabindex", "-1");

    await item.click();

    await browser.waitUntil(async () => !(await $('[role="menu"]').isExisting()), {
      timeout: 10_000,
      timeoutMsg: "Go left the list open",
    });
    await expect(await theTab()).toHaveAttribute("aria-selected", "true");
  });

  it("ignores the chat from the dropdown, and the hand goes faint again", async () => {
    await $(HAND).click();
    const ignore = await $('[role="menu"] [aria-label^="Ignore "]');
    await ignore.waitForDisplayed({ timeout: 10_000 });

    await ignore.click();

    // The core holds the ignore and answers with a queue without the chat; the chat itself
    // is still waiting, which is what an ignore is (charter-app#248).
    // The hand does not go: this chat still cannot say whether it is waiting, so it is the
    // faint hand again rather than the count.
    const faint = await untilTheHandSays(/^Nothing has asked for you, but /);
    await expect(faint).toHaveElementClass("muted");
    expect(await doing(await theTab())).toBe("waiting on you");
  });
});
