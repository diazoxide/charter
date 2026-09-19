import { browser, expect, $, $$ } from "@wdio/globals";

/**
 * The right-hand panels, against the real app started in a copy of the `daily` fixture
 * plane — with real clones in it and the forge cache a refresher would have left.
 *
 * Nothing is stubbed. The app runs git against those clones and reads that cache, and what
 * this asserts on is what a person would read off the window.
 */

/** Waits for a panel to say something, and says what it said when it never does. */
async function untilSays(testid: string, want: string | RegExp): Promise<void> {
  const panel = await $(`[data-testid="${testid}"]`);
  let last = "";
  try {
    await browser.waitUntil(
      async () => {
        last = await panel.getText();
        return typeof want === "string" ? last.includes(want) : want.test(last);
      },
      { timeout: 30_000, interval: 250 },
    );
  } catch {
    throw new Error(`${testid} never said ${want.toString()}; it said ${JSON.stringify(last)}`);
  }
}

/** Presses the button a person would read as `name`. */
async function press(name: string): Promise<void> {
  const button = await $(`button=${name}`);
  await button.waitForClickable({ timeout: 30_000 });
  await button.click();
}

/** Waits until the sidebar has read the plane, so a workspace can be focused. */
async function untilTheSidebarIsRead(): Promise<void> {
  await browser.waitUntil(
    async () => {
      const tabs = await $$('[role="tablist"][aria-label="Workspaces"] [role="tab"]').getElements();
      return (await Promise.all([...tabs].map((tab) => tab.getText()))).join(",") === "alpha,beta";
    },
    { timeout: 30_000, interval: 250, timeoutMsg: "the sidebar never listed the fixture plane" },
  );
}

describe("the workspace panels", () => {
  it("lists the focused workspace's repos with the branch each is on", async () => {
    await untilTheSidebarIsRead();

    // `alpha` is focused at the start — it is the first workspace on the plane.
    await untilSays("repo-svc", "main");
    await expect(await $('[data-testid="repo-tool"]')).toHaveText(expect.stringContaining("main"));
  });

  it("says which repo has something uncommitted and which has not", async () => {
    await untilTheSidebarIsRead();

    // The fixture leaves one untracked file in `tool` and nothing in `svc`.
    await untilSays("repo-tool", "untracked");
    await untilSays("repo-svc", "clean");
  });

  it("shows the CI state the forge cache holds, and says how old it is", async () => {
    await untilTheSidebarIsRead();

    await untilSays("ci-svc", "failed");
    await untilSays("ci-svc", "#41");
    await untilSays("ci-svc", /\d+[smh] ago/);
  });

  it("says a repo nobody has fetched is not fetched, rather than leaving it blank", async () => {
    await untilTheSidebarIsRead();

    // `tool` has no entry in the cache at all. A blank cell would read as green.
    await untilSays("ci-tool", "not fetched");
  });

  it("says the app reads CI state and never fetches it", async () => {
    await untilTheSidebarIsRead();

    await untilSays("panel-ci", "never fetches");
  });

  it("shows the focused workspace's open todos and the plane's personas", async () => {
    await untilTheSidebarIsRead();

    await untilSays("panel-todos", "Review the rollout plan");
    await untilSays("panel-personas", "steward");
    await untilSays("panel-personas", "default");
  });

  it("follows the focus to another workspace", async () => {
    await untilTheSidebarIsRead();

    await press("beta");

    // `beta` holds no repos and was never given a todo.
    await untilSays("panel-repos", "No repos in this workspace");
    await untilSays("panel-todos", "Nothing to do");

    await press("alpha");
    await untilSays("repo-svc", "main");
  });

  it("has nothing in it to press, because the panels are read-only", async () => {
    await untilTheSidebarIsRead();
    await untilSays("repo-svc", "main");

    const panels = await $('[data-testid="panels"]');
    const pressable = await panels.$$("button, input, textarea, select, a[href]").getElements();

    expect(pressable.length).toBe(0);
  });
});
