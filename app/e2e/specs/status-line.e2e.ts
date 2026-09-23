import { readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
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

/** The plane the app says it is acting on — the copy this run was given, never the repo's. */
async function planeRoot(): Promise<string> {
  const said = await $("span.plane code");
  await said.waitForDisplayed({ timeout: 30_000 });
  return (await said.getText()).trim();
}

/** Waits for the alerts button's name, and says what it was when it never gets there. */
async function untilTheButtonSays(want: string): Promise<void> {
  await $('[data-testid="status-alerts"]').waitForExist({ timeout: 20_000 });
  let last: string | null = "";
  try {
    await browser.waitUntil(
      async () => {
        last = await $('[data-testid="status-alerts"]').getAttribute("aria-label");
        return last === want;
      },
      { timeout: 30_000, interval: 250 },
    );
  } catch {
    throw new Error(`the alerts button never said ${want}; it said ${JSON.stringify(last)}`);
  }
}

/** Presses the status line's Alerts button and waits for the drawer, asked for afresh — an
 *  element looked up before the drawer existed is not the drawer. */
async function openTheDrawer(): Promise<WebdriverIO.Element> {
  await $('[data-testid="status-alerts"]').click();
  const drawer = await $('[data-testid="alerts-drawer"]');
  await drawer.waitForDisplayed({ timeout: 20_000 });
  return drawer.getElement();
}

/** Closes the drawer with Escape if it is up, and waits until it is gone. */
async function closeTheDrawer(): Promise<void> {
  if (await $('[data-testid="alerts-drawer"]').isExisting()) await browser.keys("Escape");
  await browser.waitUntil(async () => !(await $('[data-testid="alerts-drawer"]').isExisting()), {
    timeout: 20_000,
    timeoutMsg: "the alerts drawer did not close",
  });
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

  it("opens the alerts drawer over the whole window, naming every open project", async () => {
    // The fixture plane is healthy, so charter has read every project to the end and found
    // nothing — which is a claim it can make, and the button makes it as `none`, with no badge.
    await untilTheStripIsRead();
    await untilTheButtonSays("Alerts: none");
    const name = basename(await planeRoot());

    const drawer = await openTheDrawer();
    expect(await drawer.getAttribute("role")).toBe("dialog");

    // Over the window, not inside a region: the right edge to the right edge, top to bottom.
    const [width, height] = await browser.execute(() => [
      document.documentElement.clientWidth,
      document.documentElement.clientHeight,
    ]);
    const x = (await drawer.getLocation("x")) as number;
    const y = (await drawer.getLocation("y")) as number;
    expect(Math.abs(x + ((await drawer.getSize("width")) as number) - width)).toBeLessThanOrEqual(
      1,
    );
    expect(y).toBeLessThanOrEqual(1);
    expect(Math.abs(((await drawer.getSize("height")) as number) - height)).toBeLessThanOrEqual(1);

    const project = await drawer.$(`[aria-label="Alerts in ${name}"]`);
    await expect(project).toHaveText(expect.stringContaining("Nothing needs you here."));

    await closeTheDrawer();
  });

  it("lists an alert the plane has, counts it, and stops counting it once it is fixed", async () => {
    // A workspace behind the current layout: charter's `reinit` alert, with its command. The
    // drawer asks the core again as it opens, so what it lists is the plane as it is now.
    await untilTheStripIsRead();
    const plane = await planeRoot();
    const marker = join(plane, "workspaces", "beta", ".charter-structure");
    const was = readFileSync(marker, "utf8");
    try {
      writeFileSync(marker, "4\n");
      const drawer = await openTheDrawer();
      const project = await drawer.$(`[aria-label="Alerts in ${basename(plane)}"]`);
      await expect(project).toHaveText(expect.stringContaining("charter ws reinit --all"));
      await expect(project).toHaveText(expect.stringContaining("beta"));
      await closeTheDrawer();
      await untilTheButtonSays("Alerts: 1");
    } finally {
      // Put back what this spec changed, and never leave the drawer over the window: one app
      // process serves the whole run, and a scrim left up blocks every spec after this one.
      writeFileSync(marker, was);
      await closeTheDrawer();
    }
    await openTheDrawer();
    await closeTheDrawer();
    await untilTheButtonSays("Alerts: none");
  });

  it("draws no pin item for a plane that pins nothing", async () => {
    // The fixture plane pins no charter version, so `charter version` reports no drift and the
    // pin item — which is drawn only on drift — is not there at all.
    //
    // **The updater used to be asserted here beside it and has moved** to `title-bar.e2e.ts`
    // with the item itself. The two read as one pair of "version facts" and only one of them
    // is about a project: the pin is `charter version`'s verdict on THIS plane's
    // `[charter] version` (charter ADR 0030) and belongs on the line that names the project,
    // while an offer is about the app and the line is drawn once per open project.
    await untilTheStripIsRead();

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
