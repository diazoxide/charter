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

    await untilSays("worktrees-svc", "1 worktree");
    // Cut with plain git, so no charter layer — counted here, named on the explorer's row.
    await untilSays("worktrees-svc", "1 unwired");
    await untilSays("worktrees-tool", "no worktrees");
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

    await untilSays("bottom-bar", "never fetches");
  });

  it("follows the focus to another workspace", async () => {
    await onAlpha();

    await focus("beta");

    // `beta` holds no repos.
    await untilSays("bottom-bar", "No repos in this workspace");

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
   * The two cells that hold a SENTENCE — a tree charter could not read, a fetch that did not
   * happen — still wrap, and the next test is the one that holds them to it.
   */
  it("keeps a repo's row and its worktrees on one line, and scrolls sideways instead", async () => {
    await onAlpha();
    await untilSays("repo-svc", "main");

    const measured = await browser.execute(() => {
      const bar = document.querySelector<HTMLElement>('[data-testid="bottom-bar"]');
      if (!bar) throw new Error("no bottom region to narrow");
      // **The region's own box is narrowed.** The bottom region is the full width of the
      // window — its slot's size is a HEIGHT — so there is no separator that makes it narrow,
      // and the question is about this scroll container at a width narrower than its content.
      const was = bar.getAttribute("style") ?? "";
      bar.style.width = "260px";

      /** How tall one line of this element is, plus half a line and four pixels of slack — a
       *  row that wrapped is a WHOLE line taller than that, and a row that did not can still
       *  run a few pixels over its own line box because a chip in it carries a border. */
      const measure = (el: Element, what: string) => {
        const css = getComputedStyle(el);
        const line = Number.parseFloat(css.lineHeight);
        const one = Number.isFinite(line) ? line : Number.parseFloat(css.fontSize) * 1.5;
        const padding = Number.parseFloat(css.paddingTop) + Number.parseFloat(css.paddingBottom);
        return {
          what: `${what} (${(el.textContent ?? "").slice(0, 40)})`,
          height: el.getBoundingClientRect().height,
          limit: one * 1.5 + padding + 4,
        };
      };

      // `svc`'s row, and not every row: a table cell is as tall as its ROW, so a row is what
      // a wrap in any of its cells shows up in — and `tool`'s pipeline cell is the sentence
      // that is supposed to wrap, which would make its row tall for the right reason.
      const svc = bar.querySelector('[data-testid="repo-svc"]');
      const rows = [
        ...(svc === null ? [] : [measure(svc, "svc's row")]),
        ...[...bar.querySelectorAll(".worktree-tree > li")].map((li) =>
          measure(li, "a worktree row"),
        ),
      ];
      // The cells that name something, as the real engine resolved them. A rule that emitted
      // no CSS — the trap `docs/design-system.md` names — reads `normal` here.
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

    expect(measured.rows.length).toBeGreaterThan(1);
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
    // `tool` has no entry in the forge cache, so its pipeline cell says why in charter's own
    // words and at charter's own length. Held on one line it would push the region's scroll
    // out past every column it has.
    await onAlpha();
    await untilSays("ci-tool", "not fetched");

    const wraps = await browser.execute(() => {
      const cell = document.querySelector('[data-testid="ci-tool"] .none');
      return cell === null ? null : getComputedStyle(cell).whiteSpace;
    });

    expect(wraps).toBe("normal");
  });

  it("has nothing in it to press, because the bottom is what is true and not what you do", async () => {
    // charter ADR 0038's reading — *"the bottom is where you read what is true and do not
    // touch it"* — as far as a test can hold it.
    await onAlpha();
    await untilSays("repo-svc", "main");

    const bar = await $('[data-testid="bottom-bar"]');
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

  it("holds the needs-you queue, which used to be on the bar", async () => {
    await untilTheStripIsRead();

    const panels = await $('[data-testid="panels"]');
    const queue = await panels.$('[aria-label="Needs you"]');
    await queue.waitForExist({ timeout: 20_000 });
    // And it is not on the bar any more.
    const onTheBar = await $('header.bar [aria-label="Needs you"]');
    expect(await onTheBar.isExisting()).toBe(false);
  });

  it("holds no alerts section, because alerts are the window's drawer now", async () => {
    // An alert is about a plane and this region is one project's, so alerts moved to the
    // drawer the status line opens (`status-line.e2e.ts` drives it).
    await untilTheStripIsRead();
    await $('[data-testid="panels"]').waitForExist({ timeout: 20_000 });

    expect(await $('[data-testid="panel-alerts"]').isExisting()).toBe(false);
  });

  it("no longer holds the repos or the CI, which went to the bottom bar", async () => {
    await onAlpha();
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

  it("leaves the slot in the group, collapsed rather than removed", async () => {
    // charter-app#141: taking a panel out of a live group throws *"Panel constraints not found
    // for index 3"* from a document listener, so a slot that is put away stays and is collapsed
    // to nothing. jsdom never lays a group out, so this is the only place the real library is
    // asked — and a width of zero with the content gone is what "put away" has to mean.
    await untilTheStripIsRead();
    await $('[data-testid="explorer"]').waitForExist({ timeout: 20_000 });

    await (await $('button[aria-pressed="true"]=Explorer')).click();
    await browser.waitUntil(async () => !(await $('[data-testid="explorer"]').isExisting()), {
      timeout: 20_000,
      timeoutMsg: "the explorer did not go away when it was put away",
    });

    const slot = await $('[data-panel][id="region-left"]');
    await expect(slot).toBeExisting();
    expect((await slot.getSize("width")) as number).toBe(0);

    await (await $('button[aria-pressed="false"]=Explorer')).click();
    await $('[data-testid="explorer"]').waitForExist({ timeout: 20_000 });
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

    // The default arrangement (charter ADR 0038), read off the real DOM rather than off the
    // JSX: the explorer in the left slot, what is asking for you in the right, the repo state
    // along the bottom.
    await expect(await $('[data-panel][id="region-left"] [data-testid="explorer"]')).toBeExisting();
    await expect(await $('[data-panel][id="region-right"] [data-testid="panels"]')).toBeExisting();
    await expect(
      await $('[data-panel][id="region-bottom"] [data-testid="bottom-bar"]'),
    ).toBeExisting();
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

    await (await $('button[aria-pressed="true"]=Explorer')).click();
    await browser.waitUntil(async () => !(await $('[data-testid="explorer"]').isExisting()), {
      timeout: 20_000,
      timeoutMsg: "the explorer did not go away when it was put away",
    });
    await (await $('button[aria-pressed="false"]=Explorer')).click();
    await $('[data-testid="explorer"]').waitForExist({ timeout: 20_000 });

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
