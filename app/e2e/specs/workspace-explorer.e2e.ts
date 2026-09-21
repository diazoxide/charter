import { browser, expect, $, $$ } from "@wdio/globals";
import { pressAndStart } from "../opening.js";

/**
 * The left region — the repo and worktree **explorer** (charter ADR 0038) — against the real
 * app started in a copy of the `daily` fixture plane, with real clones in it and one real
 * piece cut off `svc`. Nothing here is stubbed: the app reads the files and runs git.
 *
 * This file replaces `sidebar.e2e.ts`. What that spec asserted was the old left sidebar
 * listing every workspace with its vision text, which is the duplication ADR 0038 removed —
 * the strip above is the axis, and the workspace assertions that still matter moved to the
 * strip's own queries below.
 *
 * **Its name is why it runs last, and that is deliberate.** WebdriverIO's Tauri service keeps
 * ONE app process for the whole run, so a spec that starts chats leaves them for every spec
 * after it — and `lifecycle.e2e.ts` counts what a relaunch puts back. `sidebar.e2e.ts` sorted
 * after every other spec in `wdio.conf.ts`'s glob, which is the only reason its two chats were
 * never counted by anything; calling this file `explorer.e2e.ts` sorted it FIRST and turned
 * lifecycle's "2 sessions" into 4, measured on both platforms. The name keeps the position.
 */

/** The workspaces the strip is listing, in order (charter ADR 0036).
 *
 *  The names alone: a strip tab also says how many chats are in a workspace that is not on
 *  screen, and WebdriverIO's Tauri service keeps ONE app process for the whole run, so a
 *  chat another spec started would otherwise land in this list. */
async function listed(): Promise<string[]> {
  const names = await $$(
    '[role="tablist"][aria-label="Workspaces"] [role="tab"] .workspace-name',
  ).getElements();
  return Promise.all([...names].map((name) => name.getText()));
}

/** Waits until the strip has read the plane, and says what it found if it never does. */
async function untilListed(expected: string[]): Promise<void> {
  let last: string[] = [];
  try {
    await browser.waitUntil(
      async () => {
        last = await listed();
        return last.join(",") === expected.join(",");
      },
      { timeout: 20_000, interval: 250 },
    );
  } catch {
    throw new Error(
      `the strip never listed ${JSON.stringify(expected)}; it listed ${JSON.stringify(last)}`,
    );
  }
}

/**
 * Puts the window on `alpha`, which is the workspace most of the assertions are about.
 *
 * Focused rather than assumed, for the reason in this file's own docstring: one app process
 * serves the whole run, so what this spec finds depends on what ran before it.
 */
async function onAlpha(): Promise<void> {
  await untilListed(["alpha", "beta"]);
  await focus("alpha");
}

/**
 * Focuses a workspace from the strip, which is the axis.
 *
 * By the tab's own `.workspace-name` and not by `button=<name>`: a strip tab carries counts
 * beside its name, the explorer carries a workspace row of its own, and a text match across
 * the window would pick whichever came first.
 */
async function focus(workspace: string): Promise<void> {
  const tabs = await $$('[role="tablist"][aria-label="Workspaces"] [role="tab"]').getElements();
  for (const tab of tabs) {
    if ((await tab.$(".workspace-name").getText()) === workspace) {
      await tab.click();
      return;
    }
  }
  throw new Error(`no ${workspace} on the strip; it lists ${(await listed()).join(", ")}`);
}

describe("the explorer", () => {
  it("lists the focused workspace's clones", async () => {
    await onAlpha();

    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });
    await expect(await $('[data-testid="clone-tool"]')).toBeExisting();
  });

  it("lists the worktrees cut off a clone, with the branch each is on", async () => {
    await onAlpha();

    const cut = await $('[data-testid="piece-svc-fix-login"]');
    await cut.waitForExist({ timeout: 20_000 });
    await expect(cut).toHaveText(expect.stringContaining("fix-login"));
  });

  it("says a worktree carries no charter layer before a chat is started in it", async () => {
    // The fixture's piece is cut with plain git, which is exactly the tree a chat would run
    // in with none of the plane's ask/deny rules, no persona agents and no $CHARTER_HARNESS.
    await onAlpha();

    const cut = await $('[data-testid="piece-svc-fix-login"]');
    await cut.waitForExist({ timeout: 20_000 });
    await expect(cut).toHaveText(expect.stringContaining("unwired"));
  });

  it("says a clone has no worktrees rather than drawing nothing under it", async () => {
    await onAlpha();

    const tool = await $('[data-testid="clone-tool"]');
    await browser.waitUntil(async () => (await tool.getText()).includes("No worktrees"), {
      timeout: 20_000,
      timeoutMsg: "the explorer never said whether `tool` has worktrees",
    });
  });

  it("does not list every workspace, because the strip above already answers that", async () => {
    // charter ADR 0038, and the reason this region was rewritten: the old sidebar drew every
    // workspace with its vision text under the strip that had just been made the axis.
    await onAlpha();
    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });

    const explorer = await $('[data-testid="explorer"]');
    await expect(explorer).not.toHaveText(expect.stringContaining("Retire the old importer"));
    await expect(explorer).not.toHaveText(expect.stringContaining("Ship the widget"));
  });

  it("has one thing called Workspaces in the window, and it is the strip", async () => {
    // Two `[aria-label="Workspaces"]` broke a spec when the second appeared. The old left
    // sidebar was that second one.
    await untilListed(["alpha", "beta"]);

    const named = await $$('[aria-label="Workspaces"]').getElements();
    expect(named.length).toBe(1);
    await expect(named[0]).toHaveAttribute("role", "tablist");
  });

  it("follows the focus to another workspace", async () => {
    await onAlpha();

    await focus("beta");

    // `beta` holds no repos at all.
    const explorer = await $('[data-testid="explorer"]');
    await browser.waitUntil(async () => (await explorer.getText()).includes("No repos"), {
      timeout: 20_000,
      timeoutMsg: "the explorer never followed the focus to beta",
    });

    await focus("alpha");
    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });
  });

  it("files a chat under the workspace it was started in", async () => {
    await onAlpha();
    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });

    await pressAndStart("New tab");

    // Nothing in the explorer is picked, so the chat starts in the workspace's own directory
    // and the explorer lists it there — under the workspace row, not under a piece.
    const explorer = await $('[data-testid="explorer"]');
    await browser.waitUntil(async () => (await explorer.getText()).includes("1"), {
      timeout: 20_000,
      timeoutMsg: "the chat never appeared under the workspace it was started in",
    });
  });

  it("shows one workspace's chats on the strip, and keeps the others running", async () => {
    // The axis the tmux frame had and the port lost (charter ADR 0036): the chat strip shows
    // the focused workspace's chats — and a glance at another workspace ends nothing, which
    // is the same guarantee a project behind another one has (#125).
    await untilListed(["alpha", "beta"]);
    await focus("beta");
    await pressAndStart("New tab");

    const started = await tabInFront();
    expect(started).not.toBe("");

    await focus("alpha");

    expect(await chatTabs()).not.toContain(started);

    await focus("beta");

    // Still there, still running: nothing was torn down by looking away.
    await browser.waitUntil(async () => (await chatTabs()).includes(started), {
      timeout: 20_000,
      timeoutMsg: `the chat ${started} did not come back when its workspace was focused again`,
    });
  });
});

/** The chats on the strip, which is the focused workspace's and no other's. */
async function chatTabs(): Promise<string[]> {
  const names = await $$(
    '[role="tablist"][aria-label="Tabs"] [role="tab"] .tab-name',
  ).getElements();
  return Promise.all([...names].map((name) => name.getText()));
}

/** What the tab in front is called. */
async function tabInFront(): Promise<string> {
  const name = await $(
    '[role="tablist"][aria-label="Tabs"] [role="tab"][aria-selected="true"] .tab-name',
  );
  await name.waitForExist({ timeout: 20_000 });
  return name.getText();
}
