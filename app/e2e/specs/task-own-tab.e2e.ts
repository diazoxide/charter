import { browser, expect, $$ } from "@wdio/globals";
import { READY } from "../harness.js";
import { endChat, pressAndStart } from "../opening.js";

/**
 * **A task's own tab draws a minimise and no close, and a task beside its session is two panes
 * that each say which chat they are, measured in the real WebView** (#1489).
 *
 * What is the engine's here and not jsdom's: that the button at the end of a task's tab is
 * drawn (a size, on the strip, inside its tab) and is the minimise, with no close beside it;
 * and that a split with a breadcrumb on each side overflows neither pane. Everything else,
 * what each press does and that nothing ends, is `TaskOwnTab.window.test.tsx`'s.
 *
 * **The task is pretended**, as `tab-chip.e2e.ts` pretends its own (`src/e2eTasks.ts`): a task
 * needs a persona chat and a core that dispatched it, which this suite's fake harness cannot
 * make. The window lists it, and from there its tab, its pane, its breadcrumb and their
 * buttons are the app's own. Its pane has no program behind it, so nothing is read off its
 * terminal: only its frame and its top line are measured.
 *
 * **Presses are dispatched as clicks**, for the reason `workspace-explorer.e2e.ts` gives of
 * the embedded driver's pointer.
 */

type Box = { left: number; right: number; top: number; bottom: number; width: number };

const STRIP = '[role="tablist"][aria-label="Tabs"]';
const TASK = "pretended task 1";

async function tabNames(): Promise<string[]> {
  return browser.execute(
    (strip: string) =>
      [...(document.querySelector(strip)?.querySelectorAll('[role="tab"]') ?? [])].map(
        (tab) => tab.querySelector(".tab-name")?.textContent ?? "",
      ),
    STRIP,
  );
}

async function untilShows(text: string): Promise<void> {
  await browser.waitUntil(
    async () => {
      const panes = await $$('[data-testid="pane"]').getElements();
      if (panes.length === 0) return false;
      return (await panes[0].$(".xterm-rows").getText()).includes(text);
    },
    { timeout: 30_000, interval: 250, timeoutMsg: `the pane never showed ${text}` },
  );
}

/** The chats the panes on screen show, left to right. */
async function onScreen(): Promise<number[]> {
  return browser.execute(() =>
    [...document.querySelectorAll('[data-testid="pane"]')].map((pane) =>
      Number(pane.getAttribute("data-session") ?? -1),
    ),
  );
}

/** Lists one task of session `asker`, working where `asker` works, or none. */
async function pretend(asker: number, any: boolean): Promise<void> {
  await browser.execute(
    (of: number, some: boolean, label: string) => {
      const tasks = !some
        ? []
        : [
            {
              // The root's own word: where a chat this suite starts is filed.
              workspace: "project root",
              chat: {
                session: 9100,
                name: "9100",
                cwd: null,
                harness: "claude",
                in_front: false,
                resumed: null,
                fresh: null,
                profile: null,
                persona: null,
                unreported: null,
                card: null,
                guessed: null,
                pinned: false,
                label,
                from: {
                  chat: of,
                  name: "its session",
                  workspace: "project root",
                  task: true,
                  tab: false,
                  reported: false,
                  unreported: false,
                  outcome: null,
                },
              },
            },
          ];
      window.dispatchEvent(new CustomEvent("purlis-e2e-tasks", { detail: tasks }));
    },
    asker,
    any,
    TASK,
  );
}

/** Opens the menu of the chip on the tab called `name`, by a press of its counts. */
async function openMenuOf(name: string): Promise<void> {
  await browser.execute(
    (strip: string, called: string) => {
      const cell = [...document.querySelectorAll<HTMLElement>(`${strip} > .tab`)].find(
        (one) => one.querySelector(".tab-name")?.textContent === called,
      );
      cell?.querySelector<HTMLElement>(".tab-tasks-counts")?.click();
    },
    STRIP,
    name,
  );
  await browser.waitUntil(
    async () => browser.execute(() => document.querySelector('[role="menu"].tasks-menu') !== null),
    { timeout: 10_000, interval: 100, timeoutMsg: `${name}'s task menu never opened` },
  );
}

/** Presses the button at the end of the task's line in the open menu that says `says`. */
async function place(says: string): Promise<boolean> {
  return browser.execute((words: string) => {
    const button = [
      ...document.querySelectorAll<HTMLElement>(".tasks-menu .tasks-menu-place"),
    ].find((one) => one.dataset.says?.startsWith(words));
    button?.click();
    return button !== undefined;
  }, says);
}

/** The cell of the task's own tab on the strip in front, measured; nothing while it has none. */
async function taskTab() {
  return browser.execute((strip: string) => {
    const box = (el: Element | null | undefined): Box | null => {
      if (!el) return null;
      const { left, right, top, bottom, width } = el.getBoundingClientRect();
      return { left, right, top, bottom, width };
    };
    const cell = document.querySelector<HTMLElement>(`${strip} > .tab[data-task-tab]`);
    if (!cell) return null;
    const enders = [...cell.querySelectorAll<HTMLElement>("button.closer")];
    return {
      cell: box(cell),
      name: cell.querySelector(".tab-name")?.textContent ?? "",
      mark: box(cell.querySelector('[data-mark="task"]')),
      enders: enders.map((one) => ({
        box: box(one),
        minimise: one.hasAttribute("data-minimise"),
        says: one.getAttribute("aria-label") ?? "",
        shown: getComputedStyle(one).display !== "none",
      })),
    };
  }, STRIP);
}

/** Each chat pane on screen, measured: its frame, its breadcrumb and its own controls. */
async function panes() {
  return browser.execute(() => {
    const box = (el: Element | null | undefined): Box | null => {
      if (!el) return null;
      const { left, right, top, bottom, width } = el.getBoundingClientRect();
      return { left, right, top, bottom, width };
    };
    return [...document.querySelectorAll<HTMLElement>(".pane-frame")]
      .filter((frame) => frame.querySelector('[data-testid="pane"]'))
      .map((frame) => {
        const crumbs = frame.querySelector<HTMLElement>(".pane-crumbs");
        const line = frame.querySelector<HTMLElement>(".pane-corner.at-start > .pane-chips");
        return {
          session: Number(
            frame.querySelector('[data-testid="pane"]')?.getAttribute("data-session") ?? -1,
          ),
          frame: box(frame),
          crumbs: box(crumbs),
          said: crumbs?.textContent?.replace(/\s+/g, " ").trim() ?? null,
          line: box(line),
          sideways: Math.max(frame.scrollLeft, line?.scrollLeft ?? 0),
          controls: [...frame.querySelectorAll<HTMLElement>(".pane-doing button")].map((one) => ({
            says: one.getAttribute("aria-label") ?? "",
            minimise: one.hasAttribute("data-minimise"),
            ends: one.classList.contains("ends-a-chat"),
          })),
        };
      });
  });
}

function inside(inner: Box | null, outer: Box | null, what: string) {
  if (inner === null || outer === null) throw new Error(`${what}: nothing was drawn to measure`);
  if (inner.left < outer.left - 1 || inner.right > outer.right + 1)
    throw new Error(
      `${what} is outside: ${inner.left}..${inner.right} against ${outer.left}..${outer.right}`,
    );
}

describe("a task in a tab of its own, and beside its session", () => {
  let wereAlreadyOpen: string[] = [];
  let mine = "";
  let asker = -1;

  before(async () => {
    wereAlreadyOpen = await tabNames();
    await pressAndStart("New tab");
    await untilShows(READY);
    [mine] = (await tabNames()).filter((tab) => !wereAlreadyOpen.includes(tab));
    [asker] = await onScreen();
    await pretend(asker, true);
    await browser.waitUntil(
      async () =>
        browser.execute(
          (strip: string) => document.querySelector(`${strip} .tab-tasks-counts`) !== null,
          STRIP,
        ),
      { timeout: 10_000, interval: 100, timeoutMsg: "the session's tab never wore its chip" },
    );
  });

  // One app process serves the whole run: the task's tab and pane are sent back, nothing
  // pretended is left, and the chat this file opened is ended.
  after(async () => {
    await browser.execute(() => {
      for (const one of document.querySelectorAll<HTMLElement>("[data-minimise]")) one.click();
    });
    await pretend(asker, false);
    for (let round = 0; round < 5; round++) {
      const left = (await tabNames()).filter((tab) => !wereAlreadyOpen.includes(tab));
      if (left.length === 0) break;
      for (const name of left) await endChat(`End chat ${name}`);
      await browser.pause(500);
    }
    await browser.waitUntil(
      async () => (await tabNames()).every((tab) => wereAlreadyOpen.includes(tab)),
      { timeout: 20_000, timeoutMsg: "the task-own-tab spec left a chat open behind it" },
    );
  });

  it("draws a minimise at the end of the task's own tab, and no close", async () => {
    await openMenuOf(mine);
    expect(await place("Move ")).toBe(true);
    await browser.waitUntil(async () => (await taskTab()) !== null, {
      timeout: 10_000,
      interval: 100,
      timeoutMsg: "the task never got a tab of its own",
    });

    const tab = await taskTab();
    if (tab === null) throw new Error("the task's tab is gone");
    expect(tab.name).toBe(TASK);
    // The task mark is drawn, inside the tab.
    inside(tab.mark, tab.cell, "the task mark in its tab");
    // One button at its end, the minimise, drawn with a size, inside the tab; and no close.
    expect(tab.enders).toHaveLength(1);
    const [minimise] = tab.enders;
    expect(minimise.minimise).toBe(true);
    expect(minimise.shown).toBe(true);
    expect(minimise.says.startsWith(`Send ${TASK} back`)).toBe(true);
    expect((minimise.box as Box).width).toBeGreaterThan(4);
    inside(minimise.box, tab.cell, "the minimise in its tab");

    // The minimise takes the tab away, and the session's chip still counts the task.
    await browser.execute((strip: string) => {
      document
        .querySelector<HTMLElement>(`${strip} > .tab[data-task-tab] button.closer[data-minimise]`)
        ?.click();
    }, STRIP);
    await browser.waitUntil(async () => (await taskTab()) === null, {
      timeout: 10_000,
      interval: 100,
      timeoutMsg: "the minimise left the task's tab on the strip",
    });
    expect(await tabNames()).toContain(mine);
    expect(
      await browser.execute(
        (strip: string) => document.querySelector(`${strip} .tab-tasks-counts`) !== null,
        STRIP,
      ),
    ).toBe(true);
  });

  it("shows a breadcrumb on each side of a split, and neither pane overflows", async () => {
    await openMenuOf(mine);
    expect(await place("Open ")).toBe(true);
    await browser.waitUntil(async () => (await onScreen()).length === 2, {
      timeout: 10_000,
      interval: 100,
      timeoutMsg: "the task never opened beside its session",
    });
    // No tab was added for it: it is a pane of its session's tab.
    expect(await taskTab()).toBeNull();

    const [own, task] = await panes();
    expect(own.session).toBe(asker);
    expect(task.session).toBe(9100);
    // Each side says which chat it is, in its own top line, inside its own pane.
    expect(own.said).not.toBeNull();
    expect(task.said).toContain(TASK);
    for (const pane of [own, task]) {
      inside(pane.crumbs, pane.frame, "a breadcrumb in its pane");
      inside(pane.line, pane.frame, "a top line in its pane");
      expect(pane.sideways).toBe(0);
    }
    // The panes are side by side and do not overlap.
    expect((own.frame as Box).right).toBeLessThanOrEqual((task.frame as Box).left + 1);
    // And the page itself did not grow sideways.
    expect(
      await browser.execute(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1,
      ),
    ).toBe(true);
    // The task's pane has a minimise and nothing that ends a chat; the session's has its close.
    expect(task.controls.filter((one) => one.minimise)).toHaveLength(1);
    expect(task.controls.filter((one) => one.ends)).toEqual([]);
    expect(own.controls.filter((one) => one.ends)).toHaveLength(1);
    expect(own.controls.filter((one) => one.minimise)).toEqual([]);

    // The pane's minimise sends it back, and the session's pane takes the room.
    await browser.execute(() => {
      document.querySelector<HTMLElement>(".pane-doing button[data-minimise]")?.click();
    });
    await browser.waitUntil(async () => (await onScreen()).length === 1, {
      timeout: 10_000,
      interval: 100,
      timeoutMsg: "the task's pane was not sent back",
    });
    expect(await onScreen()).toEqual([asker]);
  });
});
