import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";

/**
 * **A workspace's cross-repo changes, in a view tab of their own** (charter#470, ADR 0060),
 * against the built app and the real core.
 *
 * The change is written onto the run's plane the way `charter change create` and `add` leave it
 * — one record naming the fixture's `svc` clone — and the tab is opened the way the operator
 * opens it: `F2`, "Open changes", Enter. What is proved here and not in jsdom: the palette row
 * exists in the built window, `open_view` answers the `changes` view off the plane on disk, a
 * member charter cannot ask a forge about says why in the core's own words (the fixture's
 * clones have no `origin`, so nothing here reaches a forge or a network), and Refresh asks
 * again. What each forge answer draws is proved on stand-in `gh`/`glab` in charter-core
 * (`a_change_is_shown_with_each_members_request_and_checks.rs`).
 *
 * **It leaves the window and the plane as it found them.**
 */

const TABS = '[role="tablist"][aria-label="Tabs"]';
const PALETTE = '[role="dialog"][aria-label="Command palette"]';
const PANE = '[data-testid="view-pane-charter-changes-alpha"]';
const TITLE = "Changes · alpha";

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

/** `F2`, the row's words, Enter: the palette, with no pointer anywhere near it. */
async function runFromThePalette(row: string): Promise<void> {
  await browser.keys(["F2"]);
  await $(PALETTE).waitForDisplayed({ timeout: 20_000 });
  const box = await $("#palette-query");
  await browser.waitUntil(async () => box.isFocused(), {
    timeout: 10_000,
    timeoutMsg: "the palette did not take the keyboard when it opened",
  });
  await box.addValue(row);
  await browser.waitUntil(
    async () =>
      (await browser.execute(
        () =>
          document.querySelector('[role="option"][aria-selected="true"] .palette-title')
            ?.textContent ?? "",
      )) === row,
    { timeout: 10_000, timeoutMsg: `the palette never aimed at ${row}` },
  );
  await browser.keys(["Enter"]);
  await $(PALETTE).waitForDisplayed({ timeout: 10_000, reverse: true });
}

/** The pane, once it says `saying`. */
async function thePaneSays(saying: string): Promise<string> {
  const pane = await $(PANE);
  await pane.waitForExist({ timeout: 20_000 });
  await browser.waitUntil(async () => (await pane.getText()).includes(saying), {
    timeout: 30_000,
    timeoutMsg: `the changes tab never said ${JSON.stringify(saying)}`,
  });
  return pane.getText();
}

describe("a workspace's changes", function () {
  this.timeout(180_000);
  let record = "";

  before(async () => {
    const plane = (await ask<string[]>("open_planes"))[0];
    const dir = join(plane, "workspaces", "alpha", "changes");
    mkdirSync(dir, { recursive: true });
    record = join(dir, "api-2.json");
    writeFileSync(
      record,
      `${JSON.stringify(
        {
          change: "api-2",
          why: "bump the component API",
          created: "2026-09-26T00:00:00+00:00",
          by: "charter scenario",
          members: [{ repo: "svc", branch: "change/api-2", needs: [] }],
          excluded: [],
        },
        null,
        2,
      )}\n`,
    );
  });

  after(async () => {
    if ((await tabNames()).includes(TITLE)) {
      await $(`${TABS} button[aria-label="Close ${TITLE}"]`).click();
      await browser.waitUntil(async () => !(await tabNames()).includes(TITLE), {
        timeout: 20_000,
        timeoutMsg: "the changes tab did not close",
      });
    }
    if (record !== "") rmSync(join(record, ".."), { recursive: true, force: true });
  });

  it("opens from the palette and draws each member with what the forge could say", async () => {
    await runFromThePalette("Open changes");

    const said = await thePaneSays("api-2");
    expect(said).toContain("bump the component API");
    expect(said).toContain("svc · change/api-2");
    // The fixture's clone has no origin: the member says so, in the core's words, and is not
    // drawn as passing.
    expect(said).toContain("could not ask");
    expect(said).not.toContain("PASSED");
    expect(said).toContain("read from the forge");
    expect(await tabNames()).toContain(TITLE);
  });

  it("asks again when Refresh is pressed", async () => {
    await thePaneSays("api-2");
    // A WebDriver selector cannot mix CSS with a `button=` text match, so the pane is found
    // first and its button inside it.
    await $(PANE).$("button=Refresh").click();

    await thePaneSays("svc · change/api-2");
  });
});
