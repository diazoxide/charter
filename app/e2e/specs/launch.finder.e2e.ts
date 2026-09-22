import { browser, expect, $, $$ } from "@wdio/globals";
import { A_FINDER_LAUNCHS_PATH, READY } from "../harness.js";
import { harnessRowsDrawn, pickAndStart, pressOnly } from "../opening.js";

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
    await harnessRowsDrawn();
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

  it("has a doctor that answers for the app, with the PATH Finder gave it", async () => {
    // The incident `app/src-tauri/src/doctor.rs` exists for. The doctor was ported and CLI
    // only, so the one way to ask it was a terminal — whose shell has the operator's whole
    // PATH and cannot reproduce a Finder launch. Asked from the window, it runs in the app's
    // own process: the PATH it reports is the four-directory one, and the built-in `claude`
    // it lists is the one only charter's fixed search (charter-app#134) could find.
    const button = await $('[data-testid="status-doctor"]');
    await button.waitForExist({ timeout: 30_000 });
    await button.click();

    await expect(dialog()).toBeDisplayed();
    await browser.waitUntil(async () => (await dialog().getText()).includes("profile claude"), {
      timeout: 60_000,
      interval: 250,
      timeoutMsg: "the full doctor, opened from the window, listed no built-in claude profile",
    });
    await expect(dialog()).toHaveText(expect.stringContaining(A_FINDER_LAUNCHS_PATH));
    await expect(dialog()).toHaveText(expect.stringContaining("each harness profile probed"));

    await browser.keys(["Escape"]);
    await expect(dialog()).not.toBeDisplayed();
  });
});
