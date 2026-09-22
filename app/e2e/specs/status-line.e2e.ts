import { browser, expect, $, $$ } from "@wdio/globals";

/**
 * **charter's status line**, against the real app (`app/src/StatusLine.tsx`).
 *
 * The operator asked for the project's directory to come out of the top-right corner and into
 * a one-line bar at the very bottom of the window. Every claim in that sentence is about
 * pixels — *one* line, at the *very* bottom, under the bottom region — and **jsdom lays no
 * panel group out at all**, so `FourRegions.test.tsx` can only assert document order. This is
 * where the geometry is asked, the same split charter-app#149 made for the regions' widths.
 *
 * It changes nothing it does not put back: one app process serves the whole scenario run.
 */

/** Where the bottom edge of an element is, in the window's own coordinates. */
async function bottomOf(selector: string): Promise<number> {
  const element = await $(selector);
  await element.waitForExist({ timeout: 20_000 });
  const y = (await element.getLocation("y")) as number;
  const height = (await element.getSize("height")) as number;
  return y + height;
}

/** How tall the window's viewport is, which is what "the very bottom" is measured against. */
const viewport = (): Promise<number> =>
  browser.execute(() => document.documentElement.clientHeight);

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

/** Focuses a workspace from the strip, by the tab's own name rather than by its whole text —
 *  a strip tab carries counts beside its name. */
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

/** Waits for the status line to say something, and says what it did say when it never does. */
async function untilItSays(want: string | RegExp): Promise<void> {
  const line = await $('[data-testid="status-line"]');
  let last = "";
  try {
    await browser.waitUntil(
      async () => {
        last = await line.getText();
        return typeof want === "string" ? last.includes(want) : want.test(last);
      },
      { timeout: 30_000, interval: 250 },
    );
  } catch {
    throw new Error(
      `the status line never said ${want.toString()}; it said ${JSON.stringify(last)}`,
    );
  }
}

describe("the status line", () => {
  it("is the last thing in the window, under the bottom region", async () => {
    await untilTheStripIsRead();
    await $('[data-testid="bottom-bar"]').waitForExist({ timeout: 20_000 });

    const line = await $('[data-testid="status-line"]');
    const top = (await line.getLocation("y")) as number;
    // Nothing is below it: its bottom edge is the viewport's. A pixel of slack, because a
    // fractional layout rounds.
    expect(
      Math.abs((await bottomOf('[data-testid="status-line"]')) - (await viewport())),
    ).toBeLessThanOrEqual(1);
    // And the bottom region ends where it begins, rather than overlapping it or running past
    // it — which is the failure a document-order assertion cannot see.
    expect(await bottomOf('[data-panel][id="region-bottom"]')).toBeLessThanOrEqual(top + 1);
  });

  it("is one line tall, and takes that height from the regions rather than floating over them", async () => {
    await untilTheStripIsRead();

    const line = await $('[data-testid="status-line"]');
    const height = (await line.getSize("height")) as number;
    // One line of 0.8rem text and two small paddings. The bound is deliberately loose — this
    // is a guard against it becoming a panel, not a pin on the font metrics.
    expect(height).toBeGreaterThan(8);
    expect(height).toBeLessThanOrEqual(32);

    // The panel group stops above it. A status line laid over the regions would leave the
    // group's bottom below the line's top, and the bottom region's last row unreadable.
    const group = (await (await $(".regions")).getSize("height")) as number;
    expect(group + height).toBeLessThanOrEqual(await viewport());
  });

  it("is outside the panel group, so it is not a region", async () => {
    // `StatusLine.tsx` argues why: a slot is sized as a percentage of its group and this is
    // one line of text; a region can be put away and this must not be.
    await untilTheStripIsRead();

    const inside = await browser.execute(
      () =>
        document
          .querySelector(".regions")
          ?.contains(document.querySelector("[data-testid='status-line']")) ?? null,
    );
    expect(inside).toBe(false);
  });

  it("carries the project's directory, which is no longer on the bar", async () => {
    await untilTheStripIsRead();

    const said = await $('[data-testid="status-line"] .plane code');
    await said.waitForDisplayed({ timeout: 30_000 });
    // The run's own copy of the fixture plane, spelled as an absolute path.
    expect((await said.getText()).trim()).toMatch(/^[/\\]/);

    const onTheBar = await $("header.bar .plane");
    expect(await onTheBar.isExisting()).toBe(false);
  });

  it("says which workspace the window is on, and follows the strip", async () => {
    await untilTheStripIsRead();
    await focus("alpha");
    await untilItSays("alpha");

    await focus("beta");
    await untilItSays("beta");

    // Put back, because one app process serves every spec after this one.
    await focus("alpha");
    await untilItSays("alpha");
  });

  it("counts what the workspace holds, next to the thing it counts", async () => {
    await untilTheStripIsRead();
    await focus("alpha");

    // The fixture's `alpha` holds one todo and one real piece cut off `svc`, on a plane of two
    // workspaces. Every count is the focused workspace's except `ws`, which is the project's —
    // charter's own footer rule: a count lives next to the thing it counts.
    await untilItSays(/todo\s*1/);
    await untilItSays(/pieces\s*1/);
    await untilItSays(/ws\s*2/);
  });

  it("drops a count rather than drawing a zero", async () => {
    // `beta` holds no repos at all, so there are no worktrees to count. charter's own footer
    // drops the cell at zero — presence is the signal — and a `pieces 0` sitting there every
    // day is furniture by the end of the week.
    await untilTheStripIsRead();
    await focus("beta");
    await untilItSays("beta");

    const line = await $('[data-testid="status-line"]');
    await expect(line).not.toHaveText(expect.stringContaining("pieces"));
    await expect(line).not.toHaveText(expect.stringContaining("todo"));

    await focus("alpha");
    await untilItSays("alpha");
  });

  it("says charter cannot count alerts, on a button nothing is behind yet", async () => {
    // charter's alert row is not ported (`charter/statusline.py:_alerts`), so the button says
    // so instead of showing a zero — the same claim `Panels`'s alerts area refuses to make.
    // M6.5's drawer is what makes it pressable.
    await untilTheStripIsRead();

    const button = await $('[data-testid="status-alerts"]');
    await button.waitForExist({ timeout: 20_000 });
    expect(await button.getAttribute("aria-label")).toBe("Alerts — not drawn by this build");
    expect(await button.isEnabled()).toBe(false);
  });

  it("carries the updater as a quiet icon, and no pin item for a plane that pins nothing", async () => {
    // A scenario build never checks on its own (`charter_core::updates::checks_on_its_own`),
    // so nothing is on offer: the button is the way in to the channel, with no words. The
    // fixture plane pins no charter version, so `charter version` reports no drift and the
    // pin item — which is drawn only on drift — is not there at all.
    await untilTheStripIsRead();

    const update = await $('[data-testid="status-update"]');
    await update.waitForExist({ timeout: 20_000 });
    await browser.waitUntil(
      async () => ((await update.getAttribute("aria-label")) ?? "").includes("channel"),
      { timeout: 20_000, timeoutMsg: "the update button never said which channel it is on" },
    );
    expect(await update.getAttribute("aria-label")).toContain("nothing new known");
    expect(await $('[data-testid="status-pin"]').isExisting()).toBe(false);
  });

  it("stays at the bottom when every region is put away", async () => {
    // It is not in the arrangement, so nothing about it changes when the arrangement does —
    // and the window that is left is the panes and this line.
    await untilTheStripIsRead();
    await $('[data-testid="explorer"]').waitForExist({ timeout: 20_000 });

    const names = ["Explorer", "Attention", "State"];
    for (const name of names) await (await $(`button[aria-pressed="true"]=${name}`)).click();
    await browser.waitUntil(async () => !(await $('[data-testid="bottom-bar"]').isExisting()), {
      timeout: 20_000,
      timeoutMsg: "the bottom region did not go away when it was put away",
    });

    expect(await $('[data-testid="status-line"]').isExisting()).toBe(true);
    expect(
      Math.abs((await bottomOf('[data-testid="status-line"]')) - (await viewport())),
    ).toBeLessThanOrEqual(1);

    for (const name of names) await (await $(`button[aria-pressed="false"]=${name}`)).click();
    await $('[data-testid="bottom-bar"]').waitForExist({ timeout: 20_000 });
  });
});
