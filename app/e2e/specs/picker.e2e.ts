import { browser, expect, $, $$ } from "@wdio/globals";
import { READY } from "../harness.js";

/**
 * Picking a harness profile and a persona, against the real app in a copy of the `daily`
 * fixture plane, with a real `charter.local.toml` beside it.
 *
 * Nothing here is stubbed. The app reads the plane, probes the profile's own command to see
 * whether charter's guard would run in it, asks the operator to approve a command it has
 * never run, and only then starts anything. The profile's program is a script that answers
 * the probe as a wired Claude Code would and then runs the fake harness.
 */

async function press(name: string): Promise<void> {
  const button = await $(`button=${name}`);
  await button.waitForClickable({ timeout: 20_000 });
  await button.click();
}

const dialog = () => $('[role="dialog"]');

describe("starting a chat", () => {
  it("asks which profile and which persona, and starts nothing until a row is picked", async () => {
    await press("New tab");

    await expect(dialog()).toBeDisplayed();
    // The profile the plane declares, and the personas it has. Both come off disk.
    await expect(dialog()).toHaveText(expect.stringContaining("scenario"));
    await expect(dialog()).toHaveText(expect.stringContaining("steward"));
    // Nothing has started: no pane, and the tab bar is as it was.
    expect(await $$('[data-testid="pane"]').getElements()).toHaveLength(0);
  });

  it("shows the command of a profile charter has never run, and asks before running it", async () => {
    // The file is gitignored, so an edit to it leaves no diff for a reviewer to catch —
    // the ask is about the words that are about to run.
    await expect(dialog()).toHaveText(expect.stringContaining("claude-stand-in"));
    await expect($("button=Approve and start")).toBeDisplayed();
  });

  it("escapes without starting anything", async () => {
    await browser.keys(["Escape"]);

    await expect(dialog()).not.toBeDisplayed();
    expect(await $$('[data-testid="pane"]').getElements()).toHaveLength(0);
  });

  it("starts the chat on that profile once the operator approves its command", async () => {
    await press("New tab");
    await press("Approve and start");

    // The fake harness is running in the pane, which means the profile's command ran with
    // charter's own arguments on it and the wrapper dropped them.
    const pane = await $('[data-testid="pane"]');
    await pane.waitForDisplayed({ timeout: 30_000 });
    await browser.waitUntil(async () => (await pane.getText()).includes(READY), {
      timeout: 30_000,
      interval: 250,
      timeoutMsg: "the harness the profile names never reached the pane",
    });
  });

  it("names the profile and the persona on the chat in the sidebar", async () => {
    const sidebar = await $('nav[aria-label="Workspaces"]');
    await sidebar.waitForDisplayed({ timeout: 20_000 });

    await browser.waitUntil(
      async () => {
        const text = await sidebar.getText();
        return text.includes("scenario") && text.includes("claude");
      },
      {
        timeout: 20_000,
        interval: 250,
        timeoutMsg: "the sidebar never named the profile the chat started on",
      },
    );
  });

  it("does not ask again for a profile it has already run, exactly as it stands", async () => {
    await press("New tab");

    await expect(dialog()).toBeDisplayed();
    await expect($("button=Start")).toBeDisplayed();
    await browser.keys(["Escape"]);
  });
});
