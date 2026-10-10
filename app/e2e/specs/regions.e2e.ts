import { browser, expect, $, $$ } from "@wdio/globals";
import { textOfEach } from "../reading.js";
import { showView } from "../opening.js";

/**
 * The right region and the left side's Changes view (ADR 0038; the bottom region until #1676),
 * against the real app started in a copy of the `daily` fixture plane — with real clones in it, one real piece cut off `svc`,
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

/** Focuses a workspace from the strip, which is the axis (ADR 0036).
 *
 *  By the tab's own `.workspace-name`: a strip tab carries counts beside its name, and a
 *  `button=<name>` match across the window would pick whichever came first. */
async function focus(workspace: string): Promise<void> {
  const tabs = await $$(
    // Not the plane root's icon tab (SI-1), which has no drawn name to compare.
    '[data-strip="Workspaces"] [role="tab"]:not(.plane-root)',
  ).getElements();
  for (const tab of tabs) {
    if ((await tab.$(".workspace-name").getText()) === workspace) {
      await tab.click();
      return;
    }
  }
  throw new Error(`no ${workspace} on the workspace strip`);
}

/**
 * Puts the window on `alpha`, which is the workspace every assertion below is about.
 *
 * **Focused rather than assumed.** `alpha` is what a launch opens on, but one app process
 * serves the whole scenario run and any spec before this one may have left the window
 * somewhere else — which is exactly what happened: five of these went red for 30 s each
 * against a `beta` that holds no repos at all, on both platforms.
 */
async function onAlpha(): Promise<void> {
  await untilTheStripIsRead();
  await focus("alpha");
}

/** Waits until the plane has been read, so a workspace can be focused. */
async function untilTheStripIsRead(): Promise<void> {
  await browser.waitUntil(
    async () => {
      // In one pass: the strip is drawn while it is polled (charter#506, `reading.ts`).
      const names = await textOfEach('[data-strip="Workspaces"] [role="tab"] .workspace-name');
      return names.join(",") === "alpha,beta";
    },
    { timeout: 30_000, interval: 250, timeoutMsg: "the strip never listed the fixture plane" },
  );
}

/**
 * **The Changes view** (#1676): what the bottom region drew, the same content, as a view of the
 * left side. WebDriver reads only what is displayed, so the view is shown first, and the side
 * is left on Chats after, as it was found.
 */
describe("the Changes view", () => {
  before(async () => {
    await untilTheStripIsRead();
    await showView("Changes");
  });
  after(async () => {
    await showView("Chats");
  });

  it("lists the focused workspace's repos with the branch each is on", async () => {
    await onAlpha();

    await untilSays("repo-svc", "main");
    await expect(await $('[data-testid="repo-tool"]')).toHaveText(expect.stringContaining("main"));
  });

  it("says which repo has something uncommitted and which has not", async () => {
    await onAlpha();

    // The fixture leaves one untracked file in `tool` and nothing in `svc`.
    await untilSays("repo-tool", "untracked");
    await untilSays("repo-svc", "clean");
  });

  it("counts the worktrees cut off each clone", async () => {
    await onAlpha();

    await untilSays("worktrees-svc", "1 branch");
    // Cut with plain git, so no charter layer — counted here, named on the explorer's row.
    await untilSays("worktrees-svc", "1 unwired");
    await untilSays("worktrees-tool", "no branches");
  });

  it("shows the CI state the forge cache holds, and says how old it is", async () => {
    await onAlpha();

    await untilSays("ci-svc", "failed");
    await untilSays("ci-svc", "#41");
    await untilSays("ci-svc", /\d+[smh] ago/);
  });

  it("says a repo nobody has fetched is not fetched, rather than leaving it blank", async () => {
    await onAlpha();

    // `tool` has no entry in the cache at all. A blank cell would read as green.
    await untilSays("ci-tool", "not fetched");
  });

  it("says the app reads CI state and never fetches it", async () => {
    await untilTheStripIsRead();

    await untilSays("changes-view", "never fetches");
  });

  it("follows the focus to another workspace", async () => {
    await onAlpha();

    await focus("beta");

    // `beta` holds no repos.
    await untilSays("changes-view", "No repos in this workspace");

    await focus("alpha");
    await untilSays("repo-svc", "main");
  });

  /**
   * **The rows stay rows when the region is narrow, and jsdom cannot say so.**
   *
   * The operator asked for it of every tree in the window: a row names one thing and never
   * folds, and the region scrolls sideways instead. Down here that is the repo table's naming
   * cells and the worktree tree under each clone. A used height and a `scrollWidth` are the
   * only evidence for either, and jsdom gives every box a size of zero — so this is the only
   * place it can be asked. It also catches a rule that emitted no CSS at all.
   *
   * The one cell that holds a SENTENCE — a tree purlis could not read, a fetch that did not
   * happen — still wraps, and the next test is the one that holds it to that.
   *
   * **A cell's own height says nothing here**, which cost this spec a red run: a table cell is
   * as tall as its ROW, so the pipeline cell wrapping — as it is meant to — made every other
   * cell in that row two lines tall. What is measured is a `Range` over each cell's CONTENT,
   * whose bounding box is one line box when nothing folded and two when something did,
   * whatever the row around it is doing.
   */
  it("keeps a repo's naming cells and its worktrees on one line, and scrolls sideways", async () => {
    await onAlpha();
    await untilSays("repo-svc", "main");

    const measured = await browser.execute(() => {
      const bar = document.querySelector<HTMLElement>('[data-testid="changes-view"]');
      if (!bar) throw new Error("no Changes view to narrow");
      // **The view's own box is narrowed**, to a width narrower than its content whatever the
      // side's width is: the question is about this scroll container, not about the slot.
      const was = bar.getAttribute("style") ?? "";
      bar.style.width = "200px";

      /** One line of this element's own font, plus half a line and four pixels of slack — a
       *  row that wrapped is a WHOLE line taller than that, and a row that did not can still
       *  run a few pixels over its own line box because a chip in it carries a border. */
      const limitOf = (el: Element) => {
        const css = getComputedStyle(el);
        const line = Number.parseFloat(css.lineHeight);
        const one = Number.isFinite(line) ? line : Number.parseFloat(css.fontSize) * 1.5;
        return one * 1.5 + 4;
      };
      const named = (el: Element, what: string) =>
        `${what} (${(el.textContent ?? "").slice(0, 40)})`;

      /** How tall a cell's CONTENT is, which is not how tall the cell is. */
      const content = (el: Element, what: string) => {
        const range = document.createRange();
        range.selectNodeContents(el);
        return {
          what: named(el, what),
          height: range.getBoundingClientRect().height,
          limit: limitOf(el),
        };
      };

      const rows = [
        ...[
          ...bar.querySelectorAll(".repo-row > .repo, .repo-row > .dirt, .repo-row > .worktrees"),
        ].map((cell) => content(cell, "a naming cell")),
        // A worktree row is not a table cell, so its own box is the answer for it.
        ...[...bar.querySelectorAll(".worktree-tree > li")].map((li) => ({
          what: named(li, "a worktree row"),
          height: li.getBoundingClientRect().height,
          limit: limitOf(li) + Number.parseFloat(getComputedStyle(li).paddingTop) * 2,
        })),
      ];
      // And as the engine resolved them: a rule that emitted no CSS at all — the trap
      // `docs/design-system.md` names — reads `normal` here.
      const naming = [
        ...bar.querySelectorAll(".repo-row > .repo, .repo-row > .dirt, .repo-row > .worktrees"),
      ].map((el) => getComputedStyle(el).whiteSpace);

      const answer = {
        scrollWidth: bar.scrollWidth,
        clientWidth: bar.clientWidth,
        over: 0,
        rows,
        naming,
      };
      bar.scrollLeft = 10_000;
      answer.over = bar.scrollLeft;
      bar.scrollLeft = 0;

      bar.setAttribute("style", was);
      return answer;
    });

    expect(measured.rows.length).toBeGreaterThan(3);
    const wrapped = measured.rows
      .filter((row) => row.height > row.limit)
      .map((row) => `${row.what}: ${row.height}px, and one line of it is ${row.limit}px`);
    expect(wrapped).toEqual([]);
    expect(measured.naming.length).toBeGreaterThan(2);
    expect([...new Set(measured.naming)]).toEqual(["nowrap"]);
    expect(measured.scrollWidth).toBeGreaterThan(measured.clientWidth);
    expect(measured.over).toBeGreaterThan(0);
  });

  it("lets the cell that holds a sentence wrap, because a refusal is not a name", async () => {
    // A pipeline cell with nothing to name says why, in charter's own words and at charter's
    // own length — `not fetched — <reason>`, or `no pipeline recorded` once a refresher this
    // very run has written an entry that names none. Both are `.none`, and which of the two
    // is on screen depends on whether a refresh has landed yet, so this waits for the cell
    // rather than for either sentence. Held on one line, either would push the region's
    // horizontal scroll out past every column it has.
    await onAlpha();
    const cell = await $('[data-testid="ci-tool"] .none');
    await cell.waitForExist({ timeout: 30_000 });

    const wraps = await browser.execute(() => {
      const said = document.querySelector('[data-testid="ci-tool"] .none');
      return said === null ? null : getComputedStyle(said).whiteSpace;
    });

    expect(wraps).toBe("normal");
  });

  it("has nothing in it to press, because Changes is what is true and not what you do", async () => {
    // ADR 0038's reading — *"the bottom is where you read what is true and do not
    // touch it"*, the bottom being this view since #1676 — as far as a test can hold it.
    await onAlpha();
    await untilSays("repo-svc", "main");

    const bar = await $('[data-testid="changes-view"]');
    const pressable = await bar.$$("button, input, textarea, select, a[href]").getElements();

    expect(pressable.length).toBe(0);
  });
});

describe("the right-hand region", () => {
  it("shows the focused workspace's open todos and the plane's personas", async () => {
    await onAlpha();

    await untilSays("panel-todos", "Review the rollout plan");
    await untilSays("panel-personas", "steward");
    await untilSays("panel-personas", "default");
  });

  /**
   * **A panel is a contribution now, and this is the half only a scenario can carry**
   * (charter-app#191).
   *
   * Todos and personas stopped being hardcoded React fed by named fields: they are
   * `purlis_core::panel` values, produced in `app/src-tauri/src/panels.rs` and drawn by the
   * loop in `Panels.tsx` that draws a stranger's declared panel. The jsdom tests assert the
   * renderer against contract values and the Rust tests assert the producer against a plane on
   * disk; **neither can say that the two meet through a real command, across specta's generated
   * bindings, in a Vite build.** That is this.
   *
   * `data-panel-from` is the attribute the claim rides on, and it is deliberately one
   * ADR 0041 item 5 already wanted on screen — *show what is in force, after approval and not
   * only at it*. If the production build dropped it, an operator would have no way to tell a
   * panel his own charter draws from one an extension contributed, and this goes red.
   */
  it("draws its own panels as contributions, with the contributor named on each", async () => {
    await onAlpha();
    await untilSays("panel-todos", "Review the rollout plan");

    const whose = await browser.execute(() =>
      [...document.querySelectorAll("[data-panel-from]")].map((panel) => [
        panel.getAttribute("data-testid"),
        panel.getAttribute("data-panel-from"),
      ]),
    );

    // The fixture plane has no extension installed, so every panel on screen is charter's —
    // and every one of them still went through the seam, which is what `data-panel-from`
    // existing at all proves.
    expect(whose).toEqual([
      ["panel-todos", "charter"],
      ["panel-memory", "charter"],
      ["panel-personas", "charter"],
      ["panel-sessions", "charter"],
    ]);
  });

  // What a persona row opens — the persona's own tab — is `view-tabs.e2e.ts`.

  it("holds no needs-you queue, which is the title bar's now (charter-app#249)", async () => {
    await untilTheStripIsRead();

    const panels = await $('[data-testid="panels"]');
    await panels.waitForExist({ timeout: 20_000 });
    expect(await panels.$('[aria-label="Needs you"]').isExisting()).toBe(false);
    expect(await panels.$(".needs-you").isExisting()).toBe(false);
  });

  it("holds no alerts section, because alerts are the window's drawer now", async () => {
    // An alert is about a plane and this region is one project's, so alerts moved to the
    // drawer the status line opens (`status-line.e2e.ts` drives it).
    await untilTheStripIsRead();
    await $('[data-testid="panels"]').waitForExist({ timeout: 20_000 });

    expect(await $('[data-testid="panel-alerts"]').isExisting()).toBe(false);
  });

  it("no longer holds the repos or the CI, which are the Changes view's", async () => {
    await onAlpha();
    await $('[data-testid="repo-svc"]').waitForExist({ timeout: 30_000 });

    const panels = await $('[data-testid="panels"]');
    await expect(panels).not.toHaveText(expect.stringContaining("not fetched"));
    expect(await (await $('[data-testid="panel-repos"]')).isExisting()).toBe(false);
  });
});

/**
 * A view of the left side that is drawn: put away, the side keeps its views mounted and hidden
 * (#1673), so it is the shown one that comes and goes, not the explorer's element.
 */
const A_VIEW_SHOWN = '.region-views:not([hidden]) > [role="tabpanel"]:not([hidden])';

describe("putting a region away", () => {
  it("takes it off the window and brings it back, and never takes the panes", async () => {
    await untilTheStripIsRead();
    await $('[data-testid="explorer"]').waitForExist({ timeout: 20_000 });

    await (await $('button[aria-pressed="true"][aria-label="Navigation"]')).click();
    await browser.waitUntil(async () => !(await $(A_VIEW_SHOWN).isExisting()), {
      timeout: 20_000,
      timeoutMsg: "the left side's views did not go away when it was put away",
    });
    // The centre cannot be put away: the terminal panes are the product.
    await expect(await $('[data-strip="Tabs"]')).toBeExisting();

    await (await $('button[aria-pressed="false"][aria-label="Navigation"]')).click();
    await $(A_VIEW_SHOWN).waitForExist({ timeout: 20_000 });
  });

  it("leaves the slot in the group, collapsed rather than removed", async () => {
    // charter-app#141: taking a panel out of a live group throws *"Panel constraints not found
    // for index 3"* from a document listener, so a slot that is put away stays and is collapsed
    // to nothing. jsdom never lays a group out, so this is the only place the real library is
    // asked — and a width of zero with the content gone is what "put away" has to mean.
    await untilTheStripIsRead();
    await $('[data-testid="explorer"]').waitForExist({ timeout: 20_000 });

    await (await $('button[aria-pressed="true"][aria-label="Navigation"]')).click();
    await browser.waitUntil(async () => !(await $(A_VIEW_SHOWN).isExisting()), {
      timeout: 20_000,
      timeoutMsg: "the left side's views did not go away when it was put away",
    });

    const slot = await $('[data-panel][id="region-left"]');
    await expect(slot).toBeExisting();
    expect((await slot.getSize("width")) as number).toBe(0);

    await (await $('button[aria-pressed="false"][aria-label="Navigation"]')).click();
    await $(A_VIEW_SHOWN).waitForExist({ timeout: 20_000 });
  });
});

/**
 * **The left side's activity bar** (#1673, ADR 0038 as amended 2026-10-10), in the real window:
 * the icons switch the side between its views, a press of the open one puts the side away and
 * leaves the bar, and nothing the side draws ever lies over the terminals. What a press decides
 * and what the keys do are `FourRegions.test.tsx`'s; only a window that lays out can say where
 * things are. It leaves the side on Chats and out, as it found it.
 */
describe("the left side's activity bar", () => {
  const BAR = '[role="tablist"][aria-label="Navigation"]';
  const tab = (name: string) => $(`${BAR} [role="tab"][aria-label="${name}"]`);
  const box = (selector: string) =>
    browser.execute((where: string) => {
      const found = document.querySelector(where)?.getBoundingClientRect();
      return found ? { left: found.left, right: found.right, width: found.width } : null;
    }, selector);

  it("switches the side between its views, one view at a time", async () => {
    await untilTheStripIsRead();
    await (await tab("Chats")).waitForExist({ timeout: 20_000 });
    await expect(await $('[data-testid="chats-section"]')).toBeDisplayed();
    await expect(await $('nav[aria-label="Explorer"]')).not.toBeDisplayed();

    await (await tab("Explorer")).click();
    await $('nav[aria-label="Explorer"]').waitForDisplayed({ timeout: 20_000 });
    await expect(await $('[data-testid="chats-section"]')).not.toBeDisplayed();
    await expect(await tab("Explorer")).toHaveAttribute("aria-selected", "true");

    await (await tab("Chats")).click();
    await $('[data-testid="chats-section"]').waitForDisplayed({ timeout: 20_000 });
  });

  it("puts the side away on a press of the open view, keeps the bar, and brings it back", async () => {
    await untilTheStripIsRead();
    await $('[data-testid="chats-section"]').waitForDisplayed({ timeout: 20_000 });

    await (await tab("Chats")).click();
    const slot = await $('[data-panel][id="region-left"]');
    await browser.waitUntil(async () => ((await slot.getSize("width")) as number) === 0, {
      timeout: 20_000,
      timeoutMsg: "the left slot did not go to nothing when its open view was pressed",
    });
    await expect(await $(BAR)).toBeDisplayed();
    // Hidden, never unmounted: the list is still in the document.
    await expect(await $('[data-testid="chats-section"]')).toBeExisting();

    await (await tab("Chats")).click();
    await $('[data-testid="chats-section"]').waitForDisplayed({ timeout: 20_000 });
    expect((await slot.getSize("width")) as number).toBeGreaterThan(0);
  });

  it("stands at the window's edge, and nothing on the left lies over the terminals", async () => {
    await untilTheStripIsRead();
    await $('[data-testid="chats-section"]').waitForDisplayed({ timeout: 20_000 });

    const bar = await box(".activity-bar[data-side='left']");
    const slot = await box('[data-panel][id="region-left"]');
    const centre = await box('[data-panel][id="region-centre"]');
    expect(bar).not.toBeNull();
    expect(bar?.left ?? -1).toBeLessThanOrEqual(1);
    expect(bar?.right ?? Infinity).toBeLessThanOrEqual((slot?.left ?? 0) + 1);
    expect(slot?.right ?? Infinity).toBeLessThanOrEqual((centre?.left ?? 0) + 1);
  });
});

/**
 * **No view lies over the terminals** (#1676): every view of the left side, the right-hand
 * region and both activity bars, each laid out in the real window and measured against the
 * centre, whose panes are the product. And with nothing along the bottom any more, the centre
 * runs down to the status line: the terminals have the window's whole height.
 */
describe("the views and the terminals", () => {
  type Box = { left: number; right: number; top: number; bottom: number };
  const boxOf = (selector: string) =>
    browser.execute((where: string) => {
      const found = document.querySelector(where);
      if (found === null) return null;
      const box = found.getBoundingClientRect();
      return { left: box.left, right: box.right, top: box.top, bottom: box.bottom };
    }, selector);
  /** Whether two boxes share any area; touching edges, a pixel either way, is not overlap. */
  const overlaps = (a: Box, b: Box) =>
    Math.min(a.right, b.right) - Math.max(a.left, b.left) > 1 &&
    Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 1;

  after(async () => {
    await showView("Chats");
  });

  for (const view of ["Chats", "Explorer", "Search", "Changes"] as const) {
    it(`draws the ${view} view beside the terminals, never over them`, async () => {
      await untilTheStripIsRead();
      await showView(view);
      const panel = `.region-views > [role="tabpanel"][aria-labelledby]:not([hidden])`;
      await $(panel).waitForDisplayed({ timeout: 20_000 });

      const centre = await boxOf('[data-panel][id="region-centre"]');
      if (centre === null) throw new Error("no centre to measure against");
      const regions: [string, Box | null][] = [
        [`the ${view} view`, await boxOf(panel)],
        ["the left slot", await boxOf('[data-panel][id="region-left"]')],
        ["the right-hand region", await boxOf('[data-panel][id="region-right"]')],
        ["the left activity bar", await boxOf(".activity-bar[data-side='left']")],
      ];
      expect(regions[0][1]).not.toBeNull();
      const over = regions
        .filter((entry): entry is [string, Box] => entry[1] !== null)
        .filter(([, box]) => overlaps(box, centre))
        .map(([name, box]) => `${name} ${JSON.stringify(box)} over ${JSON.stringify(centre)}`);
      expect(over).toEqual([]);
    });
  }

  it("gives the terminals the window's whole height, down to the status line", async () => {
    await untilTheStripIsRead();
    const centre = await boxOf('[data-panel][id="region-centre"]');
    const line = await boxOf('[data-testid="status-line"]');
    if (centre === null || line === null) throw new Error("no centre or status line to measure");

    // Nothing between them: the bottom region is gone (#1676).
    await expect(await $('[data-panel][id="region-bottom"]')).not.toBeExisting();
    expect(Math.abs(line.top - centre.bottom)).toBeLessThanOrEqual(2);
  });
});

/**
 * **The arrangement drives the window** (`app/src/regions.ts`), against the real app.
 *
 * jsdom gives every element a size of zero, so `react-resizable-panels` defers its layout there
 * and no unit test can read a region's width. These are the assertions that need a window that
 * really lays out.
 */
describe("the layout as data", () => {
  it("draws each region in the slot the arrangement names", async () => {
    await untilTheStripIsRead();
    await $('[data-testid="explorer"]').waitForExist({ timeout: 20_000 });

    // The default arrangement (ADR 0038), read off the real DOM rather than off the
    // JSX: the navigation region's views in the left slot, the repo state among them since
    // #1676, and what is asking for you in the right. Nothing along the bottom.
    await expect(await $('[data-panel][id="region-left"] [data-testid="explorer"]')).toBeExisting();
    await expect(
      await $('[data-panel][id="region-left"] [data-testid="changes-view"]'),
    ).toBeExisting();
    await expect(await $('[data-panel][id="region-right"] [data-testid="panels"]')).toBeExisting();
    await expect(await $('[data-panel][id="region-bottom"]')).not.toBeExisting();
  });

  it("gives a slot the width the arrangement asks for, and not an equal share", async () => {
    // The sizes are the arrangement's — a slot laid out at a third of the window would mean the
    // library never read them. The explorer is 16% of its row by default and the right-hand
    // side 20%, so the centre is much the widest thing in the window.
    await untilTheStripIsRead();
    await $('[data-testid="explorer"]').waitForExist({ timeout: 20_000 });

    const width = async (id: string) =>
      (await (await $(`[data-panel][id="${id}"]`)).getSize("width")) as number;
    const [left, centre, right] = await Promise.all([
      width("region-left"),
      width("region-centre"),
      width("region-right"),
    ]);

    expect(centre).toBeGreaterThan(left + right);
    expect(left).toBeGreaterThan(0);
    expect(right).toBeGreaterThan(left);
  });

  it("remembers how wide a slot was dragged, and brings it back that wide", async () => {
    // What charter-app#141 deferred: only WHICH regions were drawn was remembered, never how
    // big. A drag settles, the width is written down, and putting the region away and bringing
    // it back comes back to the dragged width rather than to the library's minimum.
    await untilTheStripIsRead();
    await $('[data-testid="explorer"]').waitForExist({ timeout: 20_000 });
    const slot = await $('[data-panel][id="region-left"]');
    const before = (await slot.getSize("width")) as number;

    // The handle between the explorer and the centre, moved with the keyboard: a drag in
    // pixels is a pointer path a CI runner times differently, and the arrow keys are a resize
    // the library reports exactly as it reports a drag.
    const handle = await $('[role="separator"][aria-controls="region-left"]');
    await handle.click();
    for (let step = 0; step < 6; step++) await browser.keys(["ArrowRight"]);
    await browser.waitUntil(async () => ((await slot.getSize("width")) as number) > before, {
      timeout: 20_000,
      timeoutMsg: "the explorer's slot never got wider",
    });
    const dragged = (await slot.getSize("width")) as number;

    await (await $('button[aria-pressed="true"][aria-label="Navigation"]')).click();
    await browser.waitUntil(async () => !(await $(A_VIEW_SHOWN).isExisting()), {
      timeout: 20_000,
      timeoutMsg: "the left side's views did not go away when it was put away",
    });
    await (await $('button[aria-pressed="false"][aria-label="Navigation"]')).click();
    await $(A_VIEW_SHOWN).waitForExist({ timeout: 20_000 });

    await browser.waitUntil(
      async () => Math.abs(((await slot.getSize("width")) as number) - dragged) <= 2,
      {
        timeout: 20_000,
        timeoutMsg: `the explorer came back at a different width than the ${dragged}px it was dragged to`,
      },
    );

    // One app process serves the whole scenario run, and this spec is the only one that changes
    // a width — so it puts it back, rather than leaving every spec after it looking at a window
    // this one rearranged.
    await handle.click();
    for (let step = 0; step < 6; step++) await browser.keys(["ArrowLeft"]);
  });
});
