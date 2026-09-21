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
    // The built-in is listed only when charter can find its program (`doctor::listed`), so
    // the row being here is already the search answering.
    await expect(dialog()).toHaveText(expect.stringContaining("claude"));
  });

  it("starts the chat rather than refusing over a harness it could not look for", async () => {
    const panes = (await $$('[data-testid="pane"]').getElements()).length;
    await (await $('input[type="radio"][name="profile"]')).waitForExist({ timeout: 20_000 });
    await (await $("label*=claude")).click();

    await pickAndStart();

    const pane = await $$('[data-testid="pane"]').getElements();
    expect(pane).toHaveLength(panes + 1);
    const last = pane[pane.length - 1];
    await browser.waitUntil(async () => (await last.getText()).includes(READY), {
      timeout: 30_000,
      interval: 250,
      timeoutMsg:
        "charter-app#134: the harness in ~/.local/bin never reached the pane, so the app " +
        "launched from Finder still cannot start a chat",
    });
  });
});
