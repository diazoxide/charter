import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { endEveryChat, pressAndStart } from "../opening.js";

/**
 * **The harness capability card** (HP-19, W10), in the built app: what each harness can do,
 * read by the core off the harness's declaration and the adapter charter ships for it
 * (`charter_core::harness_card`).
 *
 * - A chat's header names its harness, and opens the harness's card as a view tab — a tab, not
 *   a dialog (the operator's 2026-09-23 ruling: new surfaces are view tabs).
 * - A control that is off because the harness lacks a capability says so in the card's line for
 *   it, followed by the card's label (ADR 0072 §3). The control is a curation row: a project
 *   whose default profile runs opencode cannot have a curation prompt typed into it
 *   (`[terminal] ready_to_type = "never"` in its declaration), so every Curate row is drawn and
 *   cannot run, and its reason is the card's.
 *
 * **It leaves the window as it found it**: the tab it opens it closes, the chat it starts it
 * ends, and the profile file it rewrites it puts back.
 */

const TABS = '[role="tablist"][aria-label="Tabs"]';
const PALETTE = '[role="dialog"][aria-label="Command palette"]';
/** The line opencode's card says for what it lacks, and the card's label after it. */
const TYPED_INTO =
  "opencode cannot have a prompt typed in for you, because charter cannot tell when it has finished starting. See What opencode can do here.";
/** What the Claude Code card's row for it says. */
const TYPED_INTO_ROW = "Can have a prompt typed in for you when it starts";

/** What the app answered a command with, insisting it answered at all. */
async function ask<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const answer = await browser.executeAsync(
    (
      name: string,
      passed: Record<string, unknown>,
      done: (out: { ok?: unknown; trouble?: string }) => void,
    ) => {
      void window.__TAURI__.core
        .invoke(name, passed)
        .then((ok) => done({ ok }))
        .catch((e: unknown) => done({ trouble: String(e) }));
    },
    command,
    args,
  );
  if (answer.trouble !== undefined) throw new Error(`${command} refused: ${answer.trouble}`);
  return answer.ok as T;
}

/** The names on the chat strip, left to right. */
async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [
      ...(document
        .querySelector('[role="tablist"][aria-label="Tabs"]')
        ?.querySelectorAll('[role="tab"]') ?? []),
    ].map((tab) => tab.textContent ?? ""),
  );
}

describe("a harness's capability card", function () {
  this.timeout(180_000);

  // The window has drawn its bar before anything here presses it: a spec run on its own is the
  // first in its app process, and the first frame can come late on a loaded machine.
  before(async () => {
    await $('button[aria-label="New tab"]').waitForClickable({ timeout: 60_000 });
  });

  it("is named in a chat's header, and opens the harness's card in a tab", async () => {
    // The plane's `scenario` profile is of kind claude: its chat runs Claude Code's card.
    await pressAndStart("New tab");
    const chip = await $(`button[aria-label="What Claude Code can do here"]`);
    await chip.waitForExist({ timeout: 30_000 });
    expect(await chip.getText()).toBe("Claude Code");

    await chip.click();

    const card = await $('[data-testid="view-pane-charter-harness-claude"]');
    await card.waitForExist({ timeout: 20_000 });
    await browser.waitUntil(async () => (await card.getText()).includes(TYPED_INTO_ROW), {
      timeout: 20_000,
      timeoutMsg: "the card tab never listed what charter can do with Claude Code",
    });
    const said = await card.getText();
    expect(said).toContain("What Claude Code can do here");
    expect(said).toContain("Tells charter when it is waiting for you");
    expect(await tabNames()).toContain("What Claude Code can do here");

    await $(`${TABS} button[aria-label="Close What Claude Code can do here"]`).click();
    await browser.waitUntil(
      async () => !(await tabNames()).includes("What Claude Code can do here"),
      {
        timeout: 20_000,
        timeoutMsg: "the card tab did not close",
      },
    );
    await endEveryChat();
  });

  describe("on a project whose default harness lacks a capability", () => {
    let local = "";
    let was = "";

    before(async () => {
      local = join((await ask<string[]>("open_planes"))[0], "charter.local.toml");
      was = readFileSync(local, "utf8");
      // The same stand-in program, declared as opencode and made the default. The program's
      // path is the scenario profile's own, read back out of the file it is in.
      const command = /command = (\[.*\])/.exec(was)?.[1];
      if (!command) throw new Error(`no command in ${local}: ${was}`);
      writeFileSync(
        local,
        `${was.replace('default = "scenario"', 'default = "typing-off"')}\n` +
          `[harness.typing-off]\nkind = "opencode"\ncommand = ${command}\n`,
      );
      await browser.execute(() => window.dispatchEvent(new Event("focus")));
    });

    after(async () => {
      if (await $(PALETTE).isExisting()) await browser.keys(["Escape"]);
      writeFileSync(local, was);
      await browser.execute(() => window.dispatchEvent(new Event("focus")));
    });

    it("draws a Curate row that cannot run, saying the card's line and label", async () => {
      let reasons: string[] = [];
      await browser
        .waitUntil(
          async () => {
            await browser.keys(["F2"]);
            const palette = await $(PALETTE);
            await palette.waitForDisplayed({ timeout: 20_000 });
            await (await palette.$("input")).addValue("Curate");
            reasons = await browser.execute(() =>
              [
                ...document.querySelectorAll(
                  '[role="dialog"][aria-label="Command palette"] [role="option"][aria-disabled="true"] .palette-why',
                ),
              ].map((why) => why.textContent ?? ""),
            );
            const found = reasons.some((why) => why.includes(TYPED_INTO));
            await browser.keys(["Escape"]);
            await expect($(PALETTE)).not.toBeDisplayed();
            return found;
          },
          { timeout: 60_000, interval: 1_000 },
        )
        .catch((why: unknown) => {
          throw new Error(
            `${String(why)} — the off Curate rows said: ${JSON.stringify(reasons.slice(0, 3))}`,
          );
        });
    });
  });
});
