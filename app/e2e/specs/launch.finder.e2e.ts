import { browser, expect, $, $$ } from "@wdio/globals";
import { READY } from "../harness.js";
import { pickAndStart, pressOnly } from "../opening.js";

/**
 * A chat started on a BUILT-IN profile, in an app launched the way Finder launches one.
 *
 * `wdio.finder.conf.ts` gives the app `PATH=/usr/bin:/bin:/usr/sbin:/sbin` and a `$HOME`
 * whose only `claude` is in `.local/bin`. Nothing is declared in the plane, so the profile
 * the picker offers is charter's own built-in, whose command is the bare word `claude`.
 *
 * Before charter-app#134 was fixed this could not get past the picker: the probe's spawn
 * failed, `wiring` answered `State::Unknown`, and the app refused with *"an unknown is not a
 * pass — nothing was started"*. Every other spec in this suite passed throughout, because
 * they all run under CI's own `PATH` against an absolute command.
 */
const dialog = () => $('[role="dialog"]');

describe("an app opened from Finder", () => {
  it("offers the built-in profile whose program only a shell's PATH would find", async () => {
    await pressOnly("New tab");

    await expect(dialog()).toBeDisplayed();
    // The built-in is listed only when charter can find its program, so the row being here is
    // already the search answering.
    await expect(dialog()).toHaveText(expect.stringContaining("claude"));

    await browser.keys(["Escape"]);
    await expect(dialog()).not.toBeDisplayed();
  });

  it("starts the chat rather than refusing over a harness it could not look for", async () => {
    const before = (await $$('[data-testid="pane"]').getElements()).length;

    await pressOnly("New tab");
    await (await $('input[type="radio"][name="profile"]')).waitForExist({ timeout: 20_000 });
    await (await $("label*=claude")).click();
    await pickAndStart();

    await browser.waitUntil(
      async () => (await $$('[data-testid="pane"]').getElements()).length === before + 1,
      {
        timeout: 30_000,
        interval: 250,
        timeoutMsg:
          "charter-app#134: no pane opened, so the app launched from Finder refused the chat",
      },
    );
    const panes = await $$('[data-testid="pane"]').getElements();
    const last = panes[panes.length - 1];
    await browser.waitUntil(async () => (await last.getText()).includes(READY), {
      timeout: 30_000,
      interval: 250,
      timeoutMsg: "charter-app#134: a pane opened but the harness in ~/.local/bin never reached it",
    });
  });
});
