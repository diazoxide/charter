import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { browser, expect, $ } from "@wdio/globals";
import { READY } from "../harness.js";
import { endChat, harnessRowsDrawn, pressOnly } from "../opening.js";

/**
 * Charter's footer inside a chat's pane, per chat, against the real app (charter ADR 0029).
 *
 * Inside a pane charter prints an empty line where its footer would go, because the app's
 * panels already draw the plane — a transposition of ADR 0019 that a port made and nobody
 * decided. ADR 0029 keeps that as the DEFAULT and gives one chat a way out of it, and both
 * halves are what this spec holds: the chat that asked carries the word, the chat beside it
 * carries nothing.
 *
 * **What is observed is the chat's ENVIRONMENT, not a drawn footer.** The decision is
 * `charter statusline`'s, made from `$CHARTER_FOOTER`, and the fake harness never runs that
 * command. So the profile's own wrapper writes the variable it was started with into the
 * plane (`harness.ts`), which is the same thing a real Claude Code's `statusLine` command
 * would have inherited. `crates/charter-cli/tests/statusline.rs` holds the other half: what
 * the command does with that word.
 *
 * **Nothing this file opens outlives it.** WebdriverIO's Tauri service keeps one app process
 * for the whole run, and `lifecycle.e2e.ts` counts every tab in the strip — so a chat left
 * behind here is a failure over there, on a session it never started. `palette.e2e.ts`
 * states the same rule; this file follows it.
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

/** The names on the tab strip. Scoped to that tablist: the workspace strip is one too. */
async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [
      ...(document
        .querySelector('[role="tablist"][aria-label="Tabs"]')
        ?.querySelectorAll('[role="tab"]') ?? []),
    ].map((tab) => tab.textContent ?? ""),
  );
}

/** Whether any pane on screen has printed `text`. A new tab shows only its OWN panes. */
async function aPaneShows(text: string): Promise<boolean> {
  const rows: string[] = await browser.execute(() =>
    [...document.querySelectorAll(".xterm-rows")].map((rows) => rows.textContent ?? ""),
  );
  return rows.some((row) => row.replace(/\s+/g, " ").includes(text));
}

/**
 * Opens a chat, ticking the footer box or leaving it alone.
 *
 * It waits on the MARKER rather than on a pane count: a new tab shows only its own panes, so
 * the number on screen does not grow with the number of chats. The marker exists only once
 * the profile's command has actually run, which is the thing this spec is about.
 */
async function startAChat(plane: string, showTheFooter: boolean): Promise<void> {
  const before = Object.keys(markers(plane)).length;
  await pressOnly("New tab");
  await dialog().waitForDisplayed({ timeout: 20_000 });
  await harnessRowsDrawn();

  // By role, not by tag: the box is a Radix checkbox, which is a `<button>` carrying the role
  // rather than an `<input>` (`docs/ui-primitives.md`). `isSelected()` reads a DOM property
  // only a real input has and would answer `false` for a ticked one, so the state is read from
  // `aria-checked` — which is what the operator's screen reader is told either way.
  const box = await $('[role="checkbox"]');
  await box.waitForExist({ timeout: 20_000 });
  expect(await box.getAttribute("aria-checked")).toBe("false");
  if (showTheFooter) await box.click();

  // Whichever button is there: the approval is recorded per profile and these specs share
  // one app process, so this must not depend on running before or after the picker's.
  const approve = await $("button=Approve and start");
  const start = (await approve.isExisting()) ? approve : await $("button=Start");
  await start.waitForClickable({ timeout: 20_000 });
  await start.click();
  await expect(dialog()).not.toBeDisplayed();

  await browser.waitUntil(async () => Object.keys(markers(plane)).length > before, {
    timeout: 30_000,
    interval: 250,
    timeoutMsg: "the chat's harness never ran",
  });
  // Settled before the next one is opened, so closing it later closes a chat that is up.
  await browser.waitUntil(() => aPaneShows(READY), {
    timeout: 30_000,
    interval: 250,
    timeoutMsg: "the harness the profile names never reached the pane",
  });
}

describe("charter's footer inside a chat", () => {
  /** The tabs that were already there, so only this file's own are closed again. */
  let wereAlreadyOpen: string[] = [];

  before(async () => {
    wereAlreadyOpen = await tabNames();
  });

  after(async () => {
    for (const name of (await tabNames()).filter((tab) => !wereAlreadyOpen.includes(tab))) {
      await endChat(`End chat ${name}`);
    }
    await browser.waitUntil(
      async () => (await tabNames()).every((tab) => wereAlreadyOpen.includes(tab)),
      { timeout: 20_000, timeoutMsg: "the footer spec left a chat open behind it" },
    );
  });

  // A failed assertion can leave the picker on screen, and one app process is shared by
  // every spec file — so the modal this file opened would fail every spec after it.
  afterEach(async () => {
    if (
      await dialog()
        .isDisplayed()
        .catch(() => false)
    ) {
      await browser.keys(["Escape"]);
    }
  });

  it("is blanked by default, and kept by the one chat that asked for it", async () => {
    const plane = await planeRoot();
    const before = markers(plane);

    await startAChat(plane, true);
    await startAChat(plane, false);

    // Only the chats this test started, so it does not depend on what ran before it.
    const mine = Object.entries(markers(plane)).filter(([name]) => !(name in before));
    expect(mine).toHaveLength(2);
    const said = mine.map(([, value]) => value).sort();
    // `show` is the one word charter writes; the other chat carries no value at all.
    expect(said).toEqual(["", "show"]);
  });
});
