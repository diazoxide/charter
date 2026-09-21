import { browser, expect, $, $$ } from "@wdio/globals";
import { READY } from "../harness.js";
import { pickAndStart, pressOnly } from "../opening.js";

/**
 * Picking a harness profile and a persona, against the real app in a copy of the `daily`
 * fixture plane, with a real `charter.local.toml` beside it.
 *
 * Nothing here is stubbed. The app reads the plane, probes the profile's own command to see
 * whether charter's guard would run in it, asks the operator to approve a command it has
 * never run, and only then starts anything. The profile's program is a wrapper that answers
 * the probe as a wired Claude Code would and then runs the fake harness — which is also the
 * shape ADR 0022 names, a program that is not called `claude`.
 *
 * Every test here is independent of the order the specs run in. They share one app process,
 * and an approval is recorded per profile, so the approval test picks a profile of its own.
 */

const dialog = () => $('[role="dialog"]');

describe("starting a chat", () => {
  it("asks which profile and which persona, and starts nothing until a row is picked", async () => {
    const panes = (await $$('[data-testid="pane"]').getElements()).length;

    await pressOnly("New tab");

    await expect(dialog()).toBeDisplayed();
    // Both come off the plane on disk: the profiles it declares, and the personas it has.
    await expect(dialog()).toHaveText(expect.stringContaining("scenario"));
    await expect(dialog()).toHaveText(expect.stringContaining("steward"));
    // Nothing has started while the question is still on screen.
    expect(await $$('[data-testid="pane"]').getElements()).toHaveLength(panes);
  });

  it("escapes having started nothing", async () => {
    const panes = (await $$('[data-testid="pane"]').getElements()).length;

    await browser.keys(["Escape"]);

    await expect(dialog()).not.toBeDisplayed();
    expect(await $$('[data-testid="pane"]').getElements()).toHaveLength(panes);
  });

  it("shows the command of a profile charter has never run, and asks before running it", async () => {
    // The file is gitignored, so an edit to it leaves no diff for a reviewer to catch —
    // which is why the ask is about the words that are about to run, not the profile's name.
    await pressOnly("New tab");
    // By role, not by tag: the rows are Radix radios, which are `<button role="radio">`
    // (`docs/ui-primitives.md`). The label is a real `<label for>` tied to one of them, so
    // clicking the words picks the row — which is the association that was missing.
    await $('[role="radio"]').waitForExist({ timeout: 20_000 });
    await (await $("label*=needs-approval")).click();

    await expect(dialog()).toHaveText(expect.stringContaining("claude-stand-in"));
    await expect($("button=Approve and start")).toBeDisplayed();

    await pickAndStart();
    const pane = await $('[data-testid="pane"]');
    await pane.waitForDisplayed({ timeout: 30_000 });
    await browser.waitUntil(async () => (await pane.getText()).includes(READY), {
      timeout: 30_000,
      interval: 250,
      timeoutMsg: "the harness the profile names never reached the pane",
    });
  });

  it("names the profile and the persona on the chat in the sidebar", async () => {
    // The profile AND its kind. The kind is the one the profile declares, not one read off
    // the program's name — the program here is `claude-stand-in`, a wrapper, and
    // `Harness::of_command` answers `None` for one exactly as it does for a shell.
    const sidebar = await $('nav[aria-label="Workspaces"]');
    await sidebar.waitForDisplayed({ timeout: 20_000 });

    let said = "";
    await browser
      .waitUntil(
        async () => {
          said = await sidebar.getText();
          return said.includes("needs-approval") && said.includes("(claude)");
        },
        {
          timeout: 20_000,
          interval: 250,
          // What it actually said, so a failure here is one somebody can act on rather than
          // one they have to reproduce.
          timeoutMsg: "the sidebar never named the profile the chat started on",
        },
      )
      .catch((why: unknown) => {
        // What it actually said, so a failure here is one somebody can act on rather than one
        // they have to reproduce.
        throw new Error(`${String(why)} — the sidebar said: ${said}`);
      });
  });

  it("does not ask again for a profile it has already run, exactly as it stands", async () => {
    await pressOnly("New tab");
    await $('[role="radio"]').waitForExist({ timeout: 20_000 });
    await (await $("label*=needs-approval")).click();

    await expect($("button=Start")).toBeDisplayed();
    await browser.keys(["Escape"]);
  });
});
