import { browser, expect, $, $$ } from "@wdio/globals";
import { pressAndStart } from "../opening.js";

/**
 * The left region — the repo and worktree **explorer** (charter ADR 0038) — against the real
 * app started in a copy of the `daily` fixture plane, with real clones in it and one real
 * piece cut off `svc`. Nothing here is stubbed: the app reads the files and runs git.
 *
 * This file replaces `sidebar.e2e.ts`. What that spec asserted was the old left sidebar
 * listing every workspace with its vision text, which is the duplication ADR 0038 removed —
 * the strip above is the axis, and the workspace assertions that still matter moved to the
 * strip's own queries below.
 *
 * **Its name is why it runs last, and that is deliberate.** WebdriverIO's Tauri service keeps
 * ONE app process for the whole run, so a spec that starts chats leaves them for every spec
 * after it — and `lifecycle.e2e.ts` counts what a relaunch puts back. `sidebar.e2e.ts` sorted
 * after every other spec in `wdio.conf.ts`'s glob, which is the only reason its two chats were
 * never counted by anything; calling this file `explorer.e2e.ts` sorted it FIRST and turned
 * lifecycle's "2 sessions" into 4, measured on both platforms. The name keeps the position.
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

/** Waits until the strip has read the plane, and says what it found if it never does. */
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
      `the strip never listed ${JSON.stringify(expected)}; it listed ${JSON.stringify(last)}`,
    );
  }
}

/**
 * Puts the window on `alpha`, which is the workspace most of the assertions are about.
 *
 * Focused rather than assumed, for the reason in this file's own docstring: one app process
 * serves the whole run, so what this spec finds depends on what ran before it.
 */
async function onAlpha(): Promise<void> {
  await untilListed(["alpha", "beta"]);
  await focus("alpha");
}

/**
 * Focuses a workspace from the strip, which is the axis.
 *
 * By the tab's own `.workspace-name` and not by `button=<name>`: a strip tab carries counts
 * beside its name, the explorer carries a workspace row of its own, and a text match across
 * the window would pick whichever came first.
 */
async function focus(workspace: string): Promise<void> {
  const tabs = await $$('[role="tablist"][aria-label="Workspaces"] [role="tab"]').getElements();
  for (const tab of tabs) {
    if ((await tab.$(".workspace-name").getText()) === workspace) {
      await tab.click();
      return;
    }
  }
  throw new Error(`no ${workspace} on the strip; it lists ${(await listed()).join(", ")}`);
}

describe("the explorer", () => {
  it("lists the focused workspace's clones", async () => {
    await onAlpha();

    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });
    await expect(await $('[data-testid="clone-tool"]')).toBeExisting();
  });

  it("lists the worktrees cut off a clone, with the branch each is on", async () => {
    await onAlpha();

    const cut = await $('[data-testid="piece-svc-fix-login"]');
    await cut.waitForExist({ timeout: 20_000 });
    await expect(cut).toHaveText(expect.stringContaining("fix-login"));
  });

  it("says a worktree carries no charter layer before a chat is started in it", async () => {
    // The fixture's piece is cut with plain git, which is exactly the tree a chat would run
    // in with none of the plane's ask/deny rules, no persona agents and no $CHARTER_HARNESS.
    await onAlpha();

    const cut = await $('[data-testid="piece-svc-fix-login"]');
    await cut.waitForExist({ timeout: 20_000 });
    await expect(cut).toHaveText(expect.stringContaining("unwired"));
  });

  it("says a clone has no worktrees rather than drawing nothing under it", async () => {
    await onAlpha();

    const tool = await $('[data-testid="clone-tool"]');
    await browser.waitUntil(async () => (await tool.getText()).includes("No worktrees"), {
      timeout: 20_000,
      timeoutMsg: "the explorer never said whether `tool` has worktrees",
    });
  });

  it("does not list every workspace, because the strip above already answers that", async () => {
    // charter ADR 0038, and the reason this region was rewritten: the old sidebar drew every
    // workspace with its vision text under the strip that had just been made the axis.
    await onAlpha();
    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });

    const explorer = await $('[data-testid="explorer"]');
    await expect(explorer).not.toHaveText(expect.stringContaining("Retire the old importer"));
    await expect(explorer).not.toHaveText(expect.stringContaining("Ship the widget"));
  });

  it("has one thing called Workspaces in the window, and it is the strip", async () => {
    // Two `[aria-label="Workspaces"]` broke a spec when the second appeared. The old left
    // sidebar was that second one.
    await untilListed(["alpha", "beta"]);

    const named = await $$('[aria-label="Workspaces"]').getElements();
    expect(named.length).toBe(1);
    await expect(named[0]).toHaveAttribute("role", "tablist");
  });

  it("follows the focus to another workspace", async () => {
    await onAlpha();

    await focus("beta");

    // `beta` holds no repos at all.
    const explorer = await $('[data-testid="explorer"]');
    await browser.waitUntil(async () => (await explorer.getText()).includes("No repos"), {
      timeout: 20_000,
      timeoutMsg: "the explorer never followed the focus to beta",
    });

    await focus("alpha");
    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });
  });

  it("files a chat under the workspace it was started in", async () => {
    await onAlpha();
    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });

    await pressAndStart("New tab");

    // Nothing in the explorer is picked, so the chat starts in the workspace's own directory
    // and the explorer lists it there — under the workspace row, not under a piece.
    const explorer = await $('[data-testid="explorer"]');
    await browser.waitUntil(async () => (await explorer.getText()).includes("1"), {
      timeout: 20_000,
      timeoutMsg: "the chat never appeared under the workspace it was started in",
    });
  });

  it("shows one workspace's chats on the strip, and keeps the others running", async () => {
    // The axis the tmux frame had and the port lost (charter ADR 0036): the chat strip shows
    // the focused workspace's chats — and a glance at another workspace ends nothing, which
    // is the same guarantee a project behind another one has (#125).
    await untilListed(["alpha", "beta"]);
    await focus("beta");
    await pressAndStart("New tab");

    const started = await tabInFront();
    expect(started).not.toBe("");

    await focus("alpha");

    expect(await chatTabs()).not.toContain(started);

    await focus("beta");

    // Still there, still running: nothing was torn down by looking away.
    await browser.waitUntil(async () => (await chatTabs()).includes(started), {
      timeout: 20_000,
      timeoutMsg: `the chat ${started} did not come back when its workspace was focused again`,
    });
  });
});

/**
 * **The tree's rows, measured — because jsdom lays nothing out.**
 *
 * Every assertion in this block is about a used value: a height in pixels, a `scrollWidth`, the
 * position an `::after` was actually painted at. `getComputedStyle` in jsdom answers from the
 * cascade it managed to build and gives every box a size of zero, so a vitest assertion that a
 * row "does not wrap" or that the region "scrolls" checks nothing at all. These need a window
 * that really lays out, which is why they are here.
 *
 * They also catch the trap `docs/design-system.md` names: a class that does not exist emits no
 * CSS and nothing goes red. A rule that never took would show up here as a row two lines tall.
 */
describe("the explorer's rows, in a region too narrow for them", () => {
  /**
   * Narrows the explorer, measures, and puts it back.
   *
   * **The region's own box is narrowed, not the separator dragged.** The question is whether
   * this scroll container folds its rows or scrolls them when it is narrower than they are, and
   * the container is `.explorer` itself — so its width is the input, whatever produced it. A
   * pointer gesture on the separator would ask the same question through a drag whose pixels
   * differ per platform, and `react-resizable-panels`' own inline style is a shape this spec
   * would then be pinning. Everything is read back inside one script, before React can render
   * again and take the style away.
   */
  async function narrowed(): Promise<{
    scrollWidth: number;
    clientWidth: number;
    scrolledTo: number;
    rows: { what: string; height: number; limit: number; overflow: number; wraps: string }[];
  }> {
    return browser.execute(() => {
      const explorer = document.querySelector<HTMLElement>('[data-testid="explorer"]');
      if (!explorer) throw new Error("no explorer to narrow");
      const was = explorer.getAttribute("style") ?? "";
      explorer.style.width = "120px";

      /** What one line of THIS row would measure, from its own font and its own padding.
       *
       *  The slack is half a line, which is the gap between the two answers rather than a
       *  guess: a row that wrapped is a WHOLE line taller, and a row that did not can still
       *  be a few pixels over its own line box because a chip inside it carries a border and
       *  its own padding. Measuring to the pixel would make this a test of `.label`'s border
       *  width; every row still clears this limit by more than three pixels on both platforms,
       *  and every wrapped one exceeds it by more than three. */
      const oneLine = (el: Element, what: string) => {
        const css = getComputedStyle(el);
        const line = Number.parseFloat(css.lineHeight);
        const height = Number.isFinite(line) ? line : Number.parseFloat(css.fontSize) * 1.5;
        const padding = Number.parseFloat(css.paddingTop) + Number.parseFloat(css.paddingBottom);
        const border =
          Number.parseFloat(css.borderTopWidth) + Number.parseFloat(css.borderBottomWidth);
        return {
          what,
          height: el.getBoundingClientRect().height,
          limit: height * 1.5 + padding + border + 4,
          // **What `min-width: max-content` is for, and the only thing that holds it.** A row
          // told only `white-space: nowrap` also stays on one line — its text simply overflows
          // its own box — and then the band behind a hovered or current row stops at the
          // region's edge while the name runs on past it. Zero here is the row's box having
          // grown to hold its content.
          overflow: el.scrollWidth - el.clientWidth,
          wraps: css.whiteSpace,
        };
      };

      const rows = [
        ...[...explorer.querySelectorAll(".spot")].map((el) => oneLine(el, "a spot")),
        ...[...explorer.querySelectorAll(".clone > summary")].map((el) => oneLine(el, "a clone")),
        ...[...explorer.querySelectorAll(".chat")].map((el) => oneLine(el, "a chat")),
        ...[...explorer.querySelectorAll(".worktree")].map((el) => oneLine(el, "a worktree")),
      ];
      const measured = {
        scrollWidth: explorer.scrollWidth,
        clientWidth: explorer.clientWidth,
        scrolledTo: 0,
        rows,
      };
      // It really scrolls, rather than merely having something to scroll: a region whose
      // overflow is hidden clips the row and answers 0 here.
      explorer.scrollLeft = 10_000;
      measured.scrolledTo = explorer.scrollLeft;
      explorer.scrollLeft = 0;

      explorer.setAttribute("style", was);
      return measured;
    });
  }

  it("keeps every row on one line, and scrolls sideways instead of folding it", async () => {
    // The operator's own report, and his screenshot: `ai-assistant` and `the workspace itself`
    // on two lines in a narrow explorer.
    await onAlpha();
    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });

    const measured = await narrowed();

    expect(measured.rows.length).toBeGreaterThan(3);
    // Reported as a list rather than asserted one at a time, so a failure names every row that
    // folded and what it measured instead of stopping at the first.
    const wrapped = measured.rows
      .filter((row) => row.height > row.limit)
      .map((row) => `${row.what}: ${row.height}px, and one line of it is ${row.limit}px`);
    expect(wrapped).toEqual([]);
    // A row's own box holds its own name, so the band behind a hovered or current row reaches
    // the end of it rather than stopping at the region's edge.
    const clipped = measured.rows
      .filter((row) => row.overflow > 1)
      .map((row) => `${row.what}: ${row.overflow}px of it is outside its own box`);
    expect(clipped).toEqual([]);
    // And the rule that says a name is never broken, as the engine resolved it. A rule that
    // emitted no CSS at all — the trap `docs/design-system.md` names — reads `normal` here.
    expect([...new Set(measured.rows.map((row) => row.wraps))]).toEqual(["nowrap"]);
    // Nothing to scroll means the rows were folded to fit instead.
    expect(measured.scrollWidth).toBeGreaterThan(measured.clientWidth);
    // And it really scrolls: a region whose overflow is hidden answers 0 here.
    expect(measured.scrolledTo).toBeGreaterThan(0);
  });

  /**
   * **The tree's elbows, against the line they are drawn for.**
   *
   * #154 draws them per row at a fixed offset (`0.9em` for a clone and a piece, `0.72em` for a
   * chat) tuned to the padding and the font size of each of those rows, and its author wrote
   * that *"any padding change misaligns them, and no test would notice"*. This is the test that
   * notices: the elbow's painted position against the middle of the name it points at, read off
   * the real WebView. A row's padding changed by 0.2rem moves one and not the other.
   */
  it("draws each elbow at the middle of the line it points at", async () => {
    await onAlpha();
    await $('[data-testid="piece-svc-fix-login"]').waitForExist({ timeout: 20_000 });

    const elbows = await browser.execute(() => {
      const explorer = document.querySelector<HTMLElement>('[data-testid="explorer"]');
      if (!explorer) throw new Error("no explorer");

      /** Where the row's `::after` — its elbow — was actually painted. */
      const elbowOf = (li: Element) => {
        const css = getComputedStyle(li, "::after");
        // The rule is written in logical properties; a computed style answers in whichever of
        // the two this engine resolves, so both are asked and the first number wins.
        for (const value of [css.insetBlockStart, css.top]) {
          const at = Number.parseFloat(value);
          if (Number.isFinite(at)) return li.getBoundingClientRect().top + at;
        }
        return Number.NaN;
      };

      const measure = (selector: string, name: string, what: string) =>
        [...explorer.querySelectorAll(selector)].flatMap((li) => {
          const label = li.querySelector(name);
          if (!label) return [];
          const box = label.getBoundingClientRect();
          return [
            {
              what: `${what} (${label.textContent ?? ""})`,
              elbow: elbowOf(li),
              middle: box.top + box.height / 2,
            },
          ];
        });

      return [
        ...measure(".clones > .clone", "summary .repo", "a clone's elbow"),
        ...measure(".pieces > li", ".spot-name", "a piece's elbow"),
        ...measure(".here > li", ".session", "a chat's elbow"),
      ];
    });

    // A run where a level drew nothing is a run that proved nothing about it.
    expect(elbows.length).toBeGreaterThan(1);
    const crooked = elbows
      .filter((at) => !(Math.abs(at.elbow - at.middle) <= 2.5))
      .map((at) => `${at.what}: drawn at ${at.elbow}px, its line centred on ${at.middle}px`);
    expect(crooked).toEqual([]);
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
