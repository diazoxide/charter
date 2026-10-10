import { browser, expect, $ } from "@wdio/globals";
import { rmSync, writeFileSync } from "node:fs";
import { endEveryChat, pressAndStart } from "../opening.js";
import { ASKED_COMMAND, ASKS_A_PERMISSION, INBOX_ALLOWED } from "../harness.js";

/**
 * **A chat asks, the person answers in the Inbox, and the chat carries on** (#1692, spec #1688).
 *
 * The whole path, with nothing faked past the harness: the reporting harness runs Codex's
 * `PermissionRequest` hook through the real binary (`ASKS_A_PERMISSION`), the hook hands the ask
 * to the app over the socket the app opened and waits, the asks registry lists it, the title
 * bar's ✋ counts it and opens the Inbox, Allow there goes back on the chat's own hook, and the
 * harness prints what it does once it is allowed. Unanswered, the hook would decide nothing and
 * the line would never be printed.
 */

/** The title bar's hand, by its test id: it opens the Inbox now, not a menu. */
const HAND = '[data-testid="title-bar"] [data-testid="needs-you-button"]';

/** The Inbox view, wherever the right side's bar draws it. */
const INBOX = '[role="tabpanel"][data-view="inbox"]';

/** The permission ask the harness raised, as the Inbox draws it. */
const ASK = `${INBOX} .inbox-ask[data-source="permission"]`;

describe("the Inbox", () => {
  before(async () => {
    // One app for the whole run: a chat an earlier spec left open would be in the count.
    await endEveryChat();
    writeFileSync(ASKS_A_PERMISSION, "");
  });

  after(async () => {
    rmSync(ASKS_A_PERMISSION, { force: true });
    await endEveryChat();
  });

  it("lists a chat's permission ask, is opened by the ✋, and answering there lets the chat go on", async () => {
    await pressAndStart("New tab");
    // The harness holds its output until a line is typed; then it asks, and waits.
    const pane = await $('[data-testid="pane"]');
    await pane.click();
    await $(".xterm-helper-textarea").addValue("\n");

    // Counted on the hand, as one thing waiting.
    await browser.waitUntil(
      async () => {
        const hand = await $(HAND);
        if (!(await hand.isExisting())) return false;
        return /^\d+ things? waits? on you$/.test((await hand.getAttribute("aria-label")) ?? "");
      },
      { timeout: 30_000, interval: 250, timeoutMsg: "the hand never counted the ask" },
    );

    await $(HAND).click();
    const ask = await $(ASK);
    await ask.waitForDisplayed({
      timeout: 20_000,
      timeoutMsg: "the ✋ did not open the Inbox on the chat's ask",
    });
    // What it asks, as text, under the chat it came from.
    await expect(ask).toHaveText(expect.stringContaining(ASKED_COMMAND));
    await expect(await ask.$("button=Go to chat")).toBeDisplayed();

    await (await ask.$("button=Allow")).click();

    // The hook printed the window's decision, and the harness went on past it.
    await browser.waitUntil(
      async () => (await (await pane.$(".xterm-rows")).getText()).includes(INBOX_ALLOWED),
      { timeout: 30_000, interval: 250, timeoutMsg: "the chat never carried on after the Allow" },
    );
    // And the ask is gone from the Inbox: its source stopped waiting.
    await browser.waitUntil(async () => !(await $(ASK).isExisting()), {
      timeout: 20_000,
      timeoutMsg: "the answered ask stayed in the Inbox",
    });
  });
});
