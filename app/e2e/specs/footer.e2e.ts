import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { browser, expect, $, $$ } from "@wdio/globals";
import { READY } from "../harness.js";
import { pressOnly } from "../opening.js";

/**
 * The harness's own footer, per chat, against the real app (charter ADR 0029).
 *
 * charter blanks Claude Code's footer inside a pane because the app already draws the plane
 * — a transposition of ADR 0019 that a port made and nobody decided. ADR 0029 keeps that as
 * the DEFAULT and gives one chat a way out of it, and both halves are what this spec holds:
 * the chat that asked carries the word, the chat beside it carries nothing.
 *
 * **What is observed is the chat's ENVIRONMENT, not a drawn footer.** The decision is
 * `charter statusline`'s, made from `$CHARTER_HARNESS_FOOTER`, and the fake harness draws no
 * footer to look at. So the profile's own wrapper writes the variable it was started with
 * into the plane (`harness.ts`), which is the same thing a real Claude Code's footer command
 * would have inherited. `crates/charter-cli/tests/statusline.rs` holds the other half: what
 * the command does with that word.
 */

const dialog = () => $('[role="dialog"]');

/** Where the profile's wrapper writes what each chat was started with. */
function markers(plane: string): Record<string, string> {
  const where = join(plane, ".charter", "scenario");
  if (!existsSync(where)) return {};
  return Object.fromEntries(
    readdirSync(where).map((name) => [name, readFileSync(join(where, name), "utf8")]),
  );
}

/** The plane the app says it is acting on — the copy this run was given, never the repo's. */
async function planeRoot(): Promise<string> {
  const said = await $("span.plane code");
  await said.waitForDisplayed({ timeout: 30_000 });
  return (await said.getText()).trim();
}

/** Opens a chat, ticking the footer box or leaving it alone, and waits for the harness. */
async function startAChat(showTheFooter: boolean): Promise<void> {
  const before = (await $$('[data-testid="pane"]').getElements()).length;
  await pressOnly("New tab");
  await dialog().waitForDisplayed({ timeout: 20_000 });
  await $('input[type="radio"][name="profile"]').waitForExist({ timeout: 20_000 });

  const box = await $('input[type="checkbox"][name="harness-footer"]');
  await box.waitForExist({ timeout: 20_000 });
  expect(await box.isSelected()).toBe(false);
  if (showTheFooter) await box.click();

  // Whichever button is there: the approval is recorded per profile and these specs share
  // one app process, so this must not depend on running before or after the picker's.
  const approve = await $("button=Approve and start");
  const start = (await approve.isExisting()) ? approve : await $("button=Start");
  await start.waitForClickable({ timeout: 20_000 });
  await start.click();
  await expect(dialog()).not.toBeDisplayed();

  await browser.waitUntil(
    async () => (await $$('[data-testid="pane"]').getElements()).length > before,
    { timeout: 30_000, interval: 250, timeoutMsg: "the chat never opened a pane" },
  );
  const panes = await $$('[data-testid="pane"]').getElements();
  const pane = panes[panes.length - 1];
  await browser.waitUntil(async () => (await pane.getText()).includes(READY), {
    timeout: 30_000,
    interval: 250,
    timeoutMsg: "the harness the profile names never reached the pane",
  });
}

describe("the harness's own footer", () => {
  it("is blanked by default, and kept by the one chat that asked for it", async () => {
    const plane = await planeRoot();
    const before = markers(plane);

    await startAChat(true);
    await startAChat(false);

    // Only the chats this test started, so it does not depend on what ran before it.
    const mine = Object.entries(markers(plane)).filter(([name]) => !(name in before));
    expect(mine).toHaveLength(2);
    const said = mine.map(([, value]) => value).sort();
    // `show` is the one word charter writes; the other chat carries no value at all.
    expect(said).toEqual(["", "show"]);
  });
});
