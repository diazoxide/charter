import { browser, expect, $, $$ } from "@wdio/globals";

/**
 * The right region and the bottom one (charter ADR 0038), against the real app started in a
 * copy of the `daily` fixture plane — with real clones in it, one real piece cut off `svc`,
 * and the forge cache a refresher would have left.
 *
 * Nothing is stubbed. The app runs git against those clones and reads that cache, and what
 * this asserts on is what a person would read off the window.
 *
 * This file replaces `panels.e2e.ts`: the repos and the CI it asserted on are still asserted
 * on, in the region they moved to.
 */

/** Waits for a region to say something, and says what it said when it never does. */
async function untilSays(testid: string, want: string | RegExp): Promise<void> {
  const region = await $(`[data-testid="${testid}"]`);
  let last = "";
  try {
    await browser.waitUntil(
      async () => {
        last = await region.getText();
        return typeof want === "string" ? last.includes(want) : want.test(last);
      },
      { timeout: 30_000, interval: 250 },
    );
  } catch {
    throw new Error(`${testid} never said ${want.toString()}; it said ${JSON.stringify(last)}`);
  }
}

/** Focuses a workspace from the strip, which is the axis (charter ADR 0036).
 *
 *  By the tab's own `.workspace-name`: a strip tab carries counts beside its name, and a
 *  `button=<name>` match across the window would pick whichever came first. */
async function focus(workspace: string): Promise<void> {
  const tabs = await $$('[role="tablist"][aria-label="Workspaces"] [role="tab"]').getElements();
  for (const tab of tabs) {
    if ((await tab.$(".workspace-name").getText()) === workspace) {
      await tab.click();
      return;
    }
  }
  throw new Error(`no ${workspace} on the workspace strip`);
}

/** Waits until the plane has been read, so a workspace can be focused. */
async function untilTheStripIsRead(): Promise<void> {
  await browser.waitUntil(
    async () => {
      const names = await $$(
        '[role="tablist"][aria-label="Workspaces"] [role="tab"] .workspace-name',
      ).getElements();
      return (
        (await Promise.all([...names].map((name) => name.getText()))).join(",") === "alpha,beta"
      );
    },
    { timeout: 30_000, interval: 250, timeoutMsg: "the strip never listed the fixture plane" },
  );
}

describe("the bottom bar", () => {
  it("lists the focused workspace's repos with the branch each is on", async () => {
    await untilTheStripIsRead();

    // `alpha` is focused at the start — it is the first workspace on the plane.
    await untilSays("repo-svc", "main");
    await expect(await $('[data-testid="repo-tool"]')).toHaveText(expect.stringContaining("main"));
  });

  it("says which repo has something uncommitted and which has not", async () => {
    await untilTheStripIsRead();

    // The fixture leaves one untracked file in `tool` and nothing in `svc`.
    await untilSays("repo-tool", "untracked");
    await untilSays("repo-svc", "clean");
  });

  it("counts the worktrees cut off each clone", async () => {
    await untilTheStripIsRead();

    await untilSays("worktrees-svc", "1 worktree");
    // Cut with plain git, so no charter layer — counted here, named on the explorer's row.
    await untilSays("worktrees-svc", "1 unwired");
    await untilSays("worktrees-tool", "no worktrees");
  });

  it("shows the CI state the forge cache holds, and says how old it is", async () => {
    await untilTheStripIsRead();

    await untilSays("ci-svc", "failed");
    await untilSays("ci-svc", "#41");
    await untilSays("ci-svc", /\d+[smh] ago/);
  });

  it("says a repo nobody has fetched is not fetched, rather than leaving it blank", async () => {
    await untilTheStripIsRead();

    // `tool` has no entry in the cache at all. A blank cell would read as green.
    await untilSays("ci-tool", "not fetched");
  });

  it("says the app reads CI state and never fetches it", async () => {
    await untilTheStripIsRead();

    await untilSays("bottom-bar", "never fetches");
  });

  it("follows the focus to another workspace", async () => {
    await untilTheStripIsRead();

    await focus("beta");

    // `beta` holds no repos.
    await untilSays("bottom-bar", "No repos in this workspace");

    await focus("alpha");
    await untilSays("repo-svc", "main");
  });

  it("has nothing in it to press, because the bottom is what is true and not what you do", async () => {
    // charter ADR 0038's reading — *"the bottom is where you read what is true and do not
    // touch it"* — as far as a test can hold it.
    await untilTheStripIsRead();
    await untilSays("repo-svc", "main");

    const bar = await $('[data-testid="bottom-bar"]');
    const pressable = await bar.$$("button, input, textarea, select, a[href]").getElements();

    expect(pressable.length).toBe(0);
  });
});

describe("the right-hand region", () => {
  it("shows the focused workspace's open todos and the plane's personas", async () => {
    await untilTheStripIsRead();

    await untilSays("panel-todos", "Review the rollout plan");
    await untilSays("panel-personas", "steward");
    await untilSays("panel-personas", "default");
  });

  it("holds the needs-you queue, which used to be on the bar", async () => {
    await untilTheStripIsRead();

    const panels = await $('[data-testid="panels"]');
    const queue = await panels.$('[aria-label="Needs you"]');
    await queue.waitForExist({ timeout: 20_000 });
    // And it is not on the bar any more.
    const onTheBar = await $('header.bar [aria-label="Needs you"]');
    expect(await onTheBar.isExisting()).toBe(false);
  });

  it("says the alert row is not ported rather than showing an empty alert area", async () => {
    // charter ADR 0038 assigns alerts here and this build has no source for one. An empty
    // area under the heading would claim charter had looked.
    await untilTheStripIsRead();

    await untilSays("panel-alerts", "Not drawn by this build");
  });

  it("no longer holds the repos or the CI, which went to the bottom bar", async () => {
    await untilTheStripIsRead();
    await untilSays("repo-svc", "main");

    const panels = await $('[data-testid="panels"]');
    await expect(panels).not.toHaveText(expect.stringContaining("not fetched"));
    expect(await (await $('[data-testid="panel-repos"]')).isExisting()).toBe(false);
  });
});

describe("putting a region away", () => {
  it("takes it off the window and brings it back, and never takes the panes", async () => {
    await untilTheStripIsRead();
    await $('[data-testid="explorer"]').waitForExist({ timeout: 20_000 });

    await (await $('button[aria-pressed="true"]=Explorer')).click();
    await browser.waitUntil(async () => !(await $('[data-testid="explorer"]').isExisting()), {
      timeout: 20_000,
      timeoutMsg: "the explorer did not go away when it was put away",
    });
    // The centre cannot be put away: the terminal panes are the product.
    await expect(await $('[role="tablist"][aria-label="Tabs"]')).toBeExisting();

    await (await $('button[aria-pressed="false"]=Explorer')).click();
    await $('[data-testid="explorer"]').waitForExist({ timeout: 20_000 });
  });
});
