import { execFileSync } from "node:child_process";
import { pressAndStart } from "../opening.js";
import { browser, expect, $ } from "@wdio/globals";
import { READY } from "../harness.js";

/**
 * The window over a day: closed to the tray with every session still running, and asked to
 * quit with what that would end named on screen.
 *
 * What is not here: pressing Quit for real. WebdriverIO's Tauri service keeps one app process
 * for every spec file in a run, so an exit here would take the rest of the run with it. That
 * half is `tools/relaunch.mjs`, which quits the app and starts it again — the only test that
 * can, because it owns the process.
 *
 * This spec runs before the others (specs run in name order) and leaves the window as it
 * found it, so nothing below depends on it having run.
 */

/** How many fake harnesses this machine is running, asked of the operating system. */
function harnessesRunning(): number {
  const ps = execFileSync("ps", ["-A", "-o", "command="], { encoding: "utf8" });
  return ps.split("\n").filter((line) => line.includes("fake-harness")).length;
}

/** Presses the button a person would read as `name`. */
async function press(name: string): Promise<void> {
  const labelled = await $(`button[aria-label="${name}"]`);
  if (await labelled.isExisting()) await labelled.click();
  else await $(`button=${name}`).click();
}

/** Whether the window is on screen, as the operating system has it. */
async function showing(): Promise<boolean> {
  return browser.executeAsync((done: (on: boolean) => void) => {
    void window.__TAURI__.core
      .invoke("window_showing")
      .then((on) => done(on === true))
      .catch(() => done(false));
  });
}

/** The names on the tab strip, left to right. Scoped to that tablist: the sidebar lists
 *  workspaces as a tablist too, and a query across the window would mix the two. */
async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [
      ...(document
        .querySelector('[role="tablist"][aria-label="Tabs"]')
        ?.querySelectorAll('[role="tab"]') ?? []),
    ].map((tab) => tab.textContent ?? ""),
  );
}

/**
 * Does one of the things the window itself can do, and insists it worked.
 *
 * A rejection is silent otherwise — a missing permission looks exactly like a window that
 * did nothing, and this spec spent a run passing green because of it. The call is a real
 * closure and never a string: the app's own content security policy forbids `eval`.
 */
async function inTheWindow(what: "close" | "show" | "ask to quit"): Promise<void> {
  const answer = await browser.executeAsync(
    (which: string, done: (out: { trouble?: string }) => void) => {
      const tauri = window.__TAURI__;
      const doing =
        which === "close"
          ? tauri.window.getCurrentWindow().close()
          : which === "show"
            ? tauri.window.getCurrentWindow().show()
            : tauri.core.invoke("ask_to_quit");
      void doing.then(() => done({})).catch((e: unknown) => done({ trouble: String(e) }));
    },
    what,
  );
  if (answer.trouble) throw new Error(`${what} failed in the window: ${answer.trouble}`);
}

/** Waits until a pane is showing `text`, and says what the panes showed instead if none does. */
async function untilAPaneShows(text: string): Promise<void> {
  let last: string[] = [];
  try {
    await browser.waitUntil(
      async () => {
        last = await browser.execute(() =>
          [...document.querySelectorAll(".xterm-rows")].map((rows) => rows.textContent ?? ""),
        );
        return last.some((rows) => rows.replace(/\s+/g, " ").includes(text));
      },
      { timeout: 30_000, interval: 250 },
    );
  } catch {
    throw new Error(
      `no pane ever showed ${JSON.stringify(text)}; ${last.length} panes showed ${JSON.stringify(
        last.map((rows) => rows.replace(/\s+/g, " ").slice(-160)),
      )}`,
    );
  }
}

describe("the window over a day", () => {
  before(async () => {
    // Two sessions, so the warning has more than one thing to name.
    await pressAndStart("New tab");
    await untilAPaneShows(READY);
    await pressAndStart("New tab");
    await untilAPaneShows(READY);
  });

  after(async () => {
    // Left as found: the next spec file shares this app process.
    for (const name of await tabNames()) await press(`End chat ${name}`);
    await browser.waitUntil(async () => (await tabNames()).length === 0, {
      timeout: 15_000,
      timeoutMsg: "the tabs this spec opened were still there",
    });
  });

  it("hides the window when it is closed, and keeps every session running", async () => {
    const running = harnessesRunning();
    expect(running).toBeGreaterThanOrEqual(2);

    // The real close path: the window's own close, which the app answers by hiding. It is
    // the same event the titlebar's button raises.
    await inTheWindow("close");

    await browser.waitUntil(async () => !(await showing()), {
      timeout: 10_000,
      timeoutMsg: "closing the window did not hide it",
    });
    expect(harnessesRunning()).toBe(running);
  });

  it("brings the window back with its sessions still in it", async () => {
    expect(await showing()).toBe(false);

    // What the tray's icon does when it is clicked. A hidden window's webview is still
    // running, so the tabs are the ones it was hidden with rather than new ones.
    await inTheWindow("show");

    await browser.waitUntil(async () => await showing(), {
      timeout: 10_000,
      timeoutMsg: "the window never came back",
    });
    expect(await tabNames()).toHaveLength(2);
    await untilAPaneShows(READY);
  });

  it("names every session it is about to end when it is asked to quit", async () => {
    await inTheWindow("ask to quit");

    const warning = await $('[role="dialog"]');
    await warning.waitForExist({ timeout: 10_000 });
    await expect(warning).toHaveText(expect.stringContaining("2 sessions will be ended"));
    for (const name of await tabNames()) {
      await expect(warning).toHaveText(expect.stringContaining(name));
    }
  });

  it("names the sessions that report no state rather than calling them idle", async () => {
    // This replaces M1.7's "charter cannot yet tell whether a session is mid-turn". It can
    // now, for a harness whose hooks say so (spec decision 3) — but the harness in THIS run
    // reports nothing, and the honest answer for one of those is still that charter cannot
    // tell. What it must never do is fold them into a reassuring "nothing is running".
    //
    // A harness that does report is the subject of `chat.state.e2e.ts`, which runs against a
    // different harness and so is a run of its own.
    const dialog = await $('[role="dialog"]');
    await expect(dialog).toHaveText(expect.stringContaining("report no state"));
    await expect(dialog).toHaveText(expect.stringContaining("mid-turn"));
    await expect(dialog).not.toHaveText(expect.stringContaining("cannot yet tell"));
  });

  it("ends nothing when the answer is no", async () => {
    const running = harnessesRunning();

    await press("Cancel");

    await browser.waitUntil(async () => !(await $('[role="dialog"]').isExisting()), {
      timeout: 10_000,
      timeoutMsg: "the warning stayed up after Cancel",
    });
    expect(harnessesRunning()).toBe(running);
    expect(await tabNames()).toHaveLength(2);
  });

  it("warns again the next time, rather than quitting outright", async () => {
    // Cancelling tells the core, so the ask is a first ask again. Without that, the second
    // Cmd-Q of the day would exit with no warning at all.
    await inTheWindow("ask to quit");

    await $('[role="dialog"]').waitForExist({ timeout: 10_000 });
    await press("Cancel");
    await browser.waitUntil(async () => !(await $('[role="dialog"]').isExisting()), {
      timeout: 10_000,
    });
  });
});
