import { browser, expect, $, $$ } from "@wdio/globals";

/**
 * The sidebar, against the real app started in a copy of the `daily` fixture plane — the
 * plane the Python charter itself wrote. Nothing here is stubbed: the app reads the files.
 */

/** The workspaces the sidebar is listing, in order. */
async function listed(): Promise<string[]> {
  const tabs = await $$('[role="tablist"][aria-label="Workspaces"] [role="tab"]').getElements();
  return Promise.all([...tabs].map((tab) => tab.getText()));
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

    await press("New tab");

    const alpha = await $('[data-testid="workspace-alpha"]');
    await browser.waitUntil(async () => (await alpha.getText()).includes("session"), {
      timeout: 20_000,
      timeoutMsg: "the chat never appeared under the workspace it was started in",
    });
    await expect(alpha).toHaveText(expect.stringContaining("workspaces/alpha"));
  });
});
