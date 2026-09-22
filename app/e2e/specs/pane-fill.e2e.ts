import { browser, expect, $, $$ } from "@wdio/globals";
import { READY } from "../harness.js";
import { pressAndStart, pressOnly } from "../opening.js";

/**
 * **A terminal fills the centre region, top to bottom** — with one pane, with a split, and
 * again after the region beside it changes size.
 *
 * The operator reported the opposite from the real app: the terminal stopped roughly 420 px
 * down and the rest of the centre was empty down to the bottom bar, with one pane and with two.
 * Every word of that is about pixels, and **jsdom lays nothing out**, so no unit test can say
 * it or say it is fixed; this is where it is asked, the way `status-line.e2e.ts` asks where the
 * status line is.
 *
 * What went wrong is worth keeping next to the test. `.panes` was `flex: 1` from the days it
 * sat in a flex column; since the four regions (charter-app#141) its parent is the centre
 * panel's inner box, which `react-resizable-panels` draws as a plain block. `flex: 1` there
 * means nothing, so `.panes` took its content's height, the pane's `height: 100%` resolved
 * against that, and xterm's `FitAddon` — which fits the terminal to its container — measured a
 * container exactly as tall as the terminal already was. The loop was closed: 24 rows because
 * the box was 24 rows tall, and the box 24 rows tall because the terminal was.
 *
 * So the assertions are made against the centre REGION, never against the pane's own box: a
 * pane that is as tall as its terminal is the bug, and measuring one against the other would
 * pass it.
 *
 * It opens its own tab, and ends every chat it opened and puts back every region it put away:
 * one app process serves the whole scenario run, and `panes.e2e.ts` — which runs straight
 * after this file, by name — takes the tab it opens to be the first thing it touches.
 */

/** The centre region's box, each pane's box, and each pane's terminal grid. */
type Measured = {
  centre: { top: number; bottom: number };
  panes: { top: number; bottom: number; screenBottom: number; rows: number; rowHeight: number }[];
};

/** Everything the assertions read, in one pass through the real layout. */
async function measure(): Promise<Measured> {
  return browser.execute(() => {
    const centre = document.querySelector('[data-panel][id="region-centre"]');
    const box = centre?.getBoundingClientRect();
    return {
      centre: { top: box?.top ?? Number.NaN, bottom: box?.bottom ?? Number.NaN },
      panes: [...document.querySelectorAll('[data-testid="pane"]')].map((pane) => {
        const own = pane.getBoundingClientRect();
        const screen = pane.querySelector(".xterm-screen")?.getBoundingClientRect();
        const rows = pane.querySelector(".xterm-rows")?.children.length ?? 0;
        const height = screen?.height ?? 0;
        return {
          top: own.top,
          bottom: own.bottom,
          screenBottom: screen?.bottom ?? Number.NaN,
          rows,
          rowHeight: rows > 0 ? height / rows : Number.NaN,
        };
      }),
    };
  });
}

/**
 * Whether every pane reaches from the top of the centre to its bottom, and whether its
 * terminal grid does too. `null` when it does; what is wrong, in words, when it does not.
 *
 * The pane is a box and should meet the region's edges to the pixel — two of slack, for a
 * fractional layout's rounding and the pane's own 1 px border. The grid is whole rows, so it
 * may stop short of the bottom by less than one row and the xterm element's few pixels of
 * padding; any more than that is a terminal that was not fitted to the room it has.
 */
function whatIsShort(now: Measured, many: number): string | null {
  if (now.panes.length !== many) return `${now.panes.length} panes on screen, not ${many}`;
  const { centre } = now;
  for (const [at, pane] of now.panes.entries()) {
    if (Math.abs(pane.top - centre.top) > 2 || Math.abs(pane.bottom - centre.bottom) > 2) {
      return `pane ${at} runs ${pane.top}–${pane.bottom}; the centre runs ${centre.top}–${centre.bottom}`;
    }
    if (!(pane.rows > 0)) return `pane ${at} has no terminal grid`;
    const slack = pane.rowHeight + 8;
    if (centre.bottom - pane.screenBottom > slack) {
      return (
        `pane ${at}'s terminal stops at ${pane.screenBottom} with ${pane.rows} rows; the centre ` +
        `ends at ${centre.bottom}, more than a row (${pane.rowHeight.toFixed(1)} px) below it`
      );
    }
  }
  return null;
}

/** Waits until the panes fill the centre. A fit lands after the session answers, so this is
 *  waited for rather than read once — and it says what it last measured when it never comes. */
async function untilTheyFill(many: number): Promise<Measured> {
  let last: Measured | undefined;
  let short: string | null = "nothing was measured";
  try {
    await browser.waitUntil(
      async () => {
        last = await measure();
        short = whatIsShort(last, many);
        return short === null;
      },
      { timeout: 20_000, interval: 250 },
    );
  } catch {
    throw new Error(`the terminal does not fill the centre region: ${short}`);
  }
  return last as Measured;
}

/** Waits until a pane's terminal shows `text`. */
async function untilShows(index: number, text: string): Promise<void> {
  await browser.waitUntil(
    async () => {
      const panes = await $$('[data-testid="pane"]').getElements();
      const pane = panes[index];
      if (pane === undefined) return false;
      return (await pane.$(".xterm-rows").getText()).includes(text);
    },
    { timeout: 30_000, interval: 250, timeoutMsg: `pane ${index} never showed ${text}` },
  );
}

/** The names on the tab strip, left to right. Scoped to that tablist: the workspaces strip is
 *  a tablist too. */
async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [
      ...(document
        .querySelector('[role="tablist"][aria-label="Tabs"]')
        ?.querySelectorAll('[role="tab"]') ?? []),
    ].map((tab) => tab.querySelector(".tab-name")?.textContent ?? ""),
  );
}

/** Brings the State region back if a failed assertion left it put away. */
async function stateRegionBack(): Promise<void> {
  const away = await $('button[aria-pressed="false"]=State');
  if (await away.isExisting()) {
    await away.click();
    await $('[data-testid="bottom-bar"]').waitForExist({ timeout: 20_000 });
  }
}

describe("the terminal in the centre region", () => {
  /** The tabs that were already there, so only this file's own are ended again. */
  let wereAlreadyOpen: string[] = [];

  before(async () => {
    wereAlreadyOpen = await tabNames();
  });

  afterEach(async () => {
    const picker = await $('[role="dialog"][aria-labelledby="start-chat"]');
    if (await picker.isDisplayed().catch(() => false)) await browser.keys(["Escape"]);
    await stateRegionBack();
  });

  // Every chat this file opened is ended. A split tab's `End chat` may leave its other pane
  // behind as a tab of its own, so this goes round until nothing of this file's is left.
  after(async () => {
    for (let round = 0; round < 5; round++) {
      const mine = (await tabNames()).filter((tab) => !wereAlreadyOpen.includes(tab));
      if (mine.length === 0) break;
      for (const name of mine) await pressOnly(`End chat ${name}`);
      await browser.pause(500);
    }
    await browser.waitUntil(
      async () => (await tabNames()).every((tab) => wereAlreadyOpen.includes(tab)),
      { timeout: 20_000, timeoutMsg: "the pane-fill spec left a chat open behind it" },
    );
  });

  it("reaches the bottom of the centre with one pane", async () => {
    await $('[data-testid="bottom-bar"]').waitForExist({ timeout: 20_000 });
    await pressAndStart("New tab");
    await untilShows(0, READY);

    const filled = await untilTheyFill(1);
    // And the centre is not itself the short thing: it runs down to the bottom region, which
    // is where the operator's screenshot showed the empty space ending.
    const bottomRegion = await browser.execute(
      () =>
        document.querySelector('[data-panel][id="region-bottom"]')?.getBoundingClientRect().top ??
        Number.NaN,
    );
    expect(Math.abs(bottomRegion - filled.centre.bottom)).toBeLessThanOrEqual(2);
  });

  it("fits again when the centre grows, and again when it shrinks back", async () => {
    // A terminal that fills on its first paint and not after a resize is the same bug: the
    // fit has to follow the box, not be taken once at mount.
    const before = await untilTheyFill(1);

    await (await $('button[aria-pressed="true"]=State')).click();
    await browser.waitUntil(async () => !(await $('[data-testid="bottom-bar"]').isExisting()), {
      timeout: 20_000,
      timeoutMsg: "the bottom region did not go away when it was put away",
    });
    const grown = await untilTheyFill(1);
    expect(grown.centre.bottom).toBeGreaterThan(before.centre.bottom);
    expect(grown.panes[0].rows).toBeGreaterThan(before.panes[0].rows);

    await (await $('button[aria-pressed="false"]=State')).click();
    await $('[data-testid="bottom-bar"]').waitForExist({ timeout: 20_000 });
    const back = await untilTheyFill(1);
    expect(back.panes[0].rows).toBeLessThan(grown.panes[0].rows);
  });

  it("reaches the bottom of the centre in both halves of a split", async () => {
    await pressAndStart("Split right");
    await untilShows(1, READY);

    await untilTheyFill(2);
  });
});
