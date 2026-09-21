import { browser, expect, $, $$ } from "@wdio/globals";
import { pressAndStart } from "../opening.js";

/**
 * The sidebar, against the real app started in a copy of the `daily` fixture plane — the
 * plane the Python charter itself wrote. Nothing here is stubbed: the app reads the files.
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

/** Waits until the sidebar has read the plane, and says what it found if it never does. */
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
      `the sidebar never listed ${JSON.stringify(expected)}; it listed ${JSON.stringify(last)}`,
    );
  }
}

/** Presses the button a person would read as `name`. */
async function press(name: string): Promise<void> {
  const button = await $(`button=${name}`);
  await button.waitForClickable({ timeout: 20_000 });
  await button.click();
}

describe("the sidebar", () => {
  it("lists every workspace on the plane, with what each is for", async () => {
    await untilListed(["alpha", "beta"]);

    const alpha = await $('[data-testid="workspace-alpha"]');
    const beta = await $('[data-testid="workspace-beta"]');
    await expect(alpha).toHaveText(expect.stringContaining("Ship the widget"));
    await expect(beta).toHaveText(expect.stringContaining("Retire the old importer"));
  });

  it("shows the focused workspace's open todos and the persona a chat would adopt", async () => {
    await untilListed(["alpha", "beta"]);

    const focused = await $('[data-testid="focused"]');
    // `alpha` has exactly one todo left open in the fixture: the other was closed, and
    // closing deletes the file.
    await expect(focused).toHaveText(expect.stringContaining("Review the rollout plan"));
    await expect(focused).toHaveText(expect.stringContaining("steward"));
  });

  it("shows another workspace's todos when that workspace is focused", async () => {
    await untilListed(["alpha", "beta"]);

    await press("beta");

    const focused = await $('[data-testid="focused"]');
    // `beta` was never given a todo, so it has no `todos/` at all.
    await expect(focused).toHaveText(expect.stringContaining("Nothing to do"));
    await expect(focused).not.toHaveText(expect.stringContaining("Review the rollout plan"));
  });

  it("files a chat under the workspace it was started in", async () => {
    await untilListed(["alpha", "beta"]);
    await press("alpha");

    await pressAndStart("New tab");

    // The chat is listed under `alpha` by the directory it works in — which is the only
    // thing relating a chat to a workspace, since nothing on the plane records one. It is
    // shown by the name the tab carries, not by its session id.
    const alpha = await $('[data-testid="workspace-alpha"]');
    await browser.waitUntil(async () => (await alpha.getText()).includes("workspaces/alpha"), {
      timeout: 20_000,
      timeoutMsg: "the chat never appeared under the workspace it was started in",
    });
    await expect(alpha).toHaveText(expect.stringContaining("1"));
    await expect(await $('[data-testid="unfiled"]')).not.toBeExisting();
  });

  it("shows one workspace's chats on the strip, and keeps the others running", async () => {
    // The axis the tmux frame had and the port lost (charter ADR 0036): a top-level tab
    // there was a WORKSPACE and the sessions lived under it. Here the chat strip shows the
    // focused workspace's chats — and a glance at another workspace ends nothing, which is
    // the same guarantee a project behind another one has (#125).
    await untilListed(["alpha", "beta"]);
    await press("beta");
    await pressAndStart("New tab");

    const started = await tabInFront();
    expect(started).not.toBe("");

    await press("alpha");

    expect(await chatTabs()).not.toContain(started);

    await press("beta");

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
