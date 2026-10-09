import { browser, expect, $$ } from "@wdio/globals";
import { READY } from "../harness.js";
import { endChat, endEveryChat, pressAndStart } from "../opening.js";

/**
 * **A session's tab wears a chip of its tasks, measured and driven in the real WebView**
 * (#1487).
 *
 * Three things here are the engine's and not jsdom's: how wide a tab is with and without a
 * chip, what gives way on a crowded strip, and what real timers do with a pointer that crosses
 * the strip against one that rests on a chip. `TabChip.window.test.tsx` holds everything else,
 * on fake clocks.
 *
 * **The tasks are pretended.** A task needs a persona chat and a core that dispatched it, which
 * this suite's fake harness cannot make (`pane-crumbs.e2e.ts` says the same). A chip is what it
 * does under a pointer, so it cannot be drawn by hand as that spec draws its breadcrumb: the
 * `e2e` build listens for the chats to list as tasks (`src/e2eTasks.ts`), and the chip, its
 * menu and their timers are the app's own from there.
 *
 * **The pointer is dispatched, not performed**, for the reason `workspace-explorer.e2e.ts`
 * gives: a WebDriver pointer action on the embedded driver raises no pointer events the page
 * sees. What is sent is what the platform sends as a pointer goes from one element to the
 * next: it leaves the one for the other, and moves.
 */

type Box = { left: number; right: number; top: number; bottom: number; width: number };

/** Where the pretended tasks are said to work. Not the session's workspace, so every task's
 *  row also says where: the widest a row gets. */
const ELSEWHERE = "another-workspace";

const STRIP = '[data-strip="Tabs"]';

async function tabNames(): Promise<string[]> {
  return browser.execute(
    (strip: string) =>
      [...(document.querySelector(strip)?.querySelectorAll('[role="tab"]') ?? [])].map(
        (tab) => tab.querySelector(".tab-name")?.textContent ?? "",
      ),
    STRIP,
  );
}

/** The strip as it is drawn: every tab's name, the one in front, and the chats on screen. */
async function strip(): Promise<{ tabs: string[]; front: string | null; panes: number[] }> {
  return browser.execute((at: string) => {
    const tabs = [...(document.querySelector(at)?.querySelectorAll('[role="tab"]') ?? [])];
    const name = (tab: Element) => tab.querySelector(".tab-name")?.textContent ?? "";
    const front = tabs.find((tab) => tab.getAttribute("aria-selected") === "true");
    return {
      tabs: tabs.map(name),
      front: front ? name(front) : null,
      panes: [...document.querySelectorAll('[data-testid="pane"]')].map((pane) =>
        Number(pane.getAttribute("data-session") ?? -1),
      ),
    };
  }, STRIP);
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

/** The number of the chat the pane in front shows. */
async function sessionInFront(): Promise<number> {
  return browser.execute(() =>
    Number(document.querySelector('[data-testid="pane"]')?.getAttribute("data-session") ?? -1),
  );
}

/** Presses the tab called `name`, as a click. */
async function select(name: string): Promise<void> {
  await browser.execute(
    (strip: string, called: string) => {
      const tab = [...(document.querySelector(strip)?.querySelectorAll('[role="tab"]') ?? [])].find(
        (one) => one.querySelector(".tab-name")?.textContent === called,
      );
      (tab as HTMLElement | undefined)?.click();
    },
    STRIP,
    name,
  );
}

/**
 * Says which chats the window lists as tasks of session `asker`: `how` is one entry per task,
 * `working`, `failed` or `done`. An empty list takes every pretended task away.
 */
async function pretend(asker: number, how: ("working" | "failed" | "done")[]): Promise<void> {
  await browser.execute(
    (of: number, each: string[], workspace: string) => {
      const tasks = each.map((outcome, at) => ({
        workspace,
        chat: {
          session: 9000 + at,
          name: String(9000 + at),
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
          label: `pretended task ${at + 1}`,
          from: {
            chat: of,
            name: "its session",
            workspace,
            task: true,
            tab: false,
            reported: outcome !== "working",
            unreported: false,
            outcome: outcome === "working" ? null : outcome,
          },
        },
      }));
      window.dispatchEvent(new CustomEvent("purlis-e2e-tasks", { detail: tasks }));
    },
    asker,
    how,
    ELSEWHERE,
  );
}

/** Every drawn tab's cell, by its name: its box, and whether it wears a chip. */
async function cells(): Promise<Record<string, { box: Box; chip: boolean }>> {
  return browser.execute((strip: string) => {
    const out: Record<string, { box: Box; chip: boolean }> = {};
    for (const cell of document.querySelectorAll<HTMLElement>(`${strip} > .tab`)) {
      const name = cell.querySelector(".tab-name")?.textContent;
      if (!name) continue;
      const { left, right, top, bottom, width } = cell.getBoundingClientRect();
      out[name] = {
        box: { left, right, top, bottom, width },
        chip: cell.querySelector(".tab-tasks") !== null,
      };
    }
    return out;
  }, STRIP);
}

/** What the cell of the tab called `name` holds, measured. */
async function measured(name: string) {
  return browser.execute(
    (strip: string, called: string) => {
      const box = (el: Element | null | undefined): Box | null => {
        if (!el) return null;
        const { left, right, top, bottom, width } = el.getBoundingClientRect();
        return { left, right, top, bottom, width };
      };
      const cell = [...document.querySelectorAll<HTMLElement>(`${strip} > .tab`)].find(
        (one) => one.querySelector(".tab-name")?.textContent === called,
      );
      const chip = cell?.querySelector<HTMLElement>(".tab-tasks");
      const counts = chip?.querySelector<HTMLElement>(".tab-tasks-counts");
      const label = cell?.querySelector<HTMLElement>(".tab-name");
      const rem = Number.parseFloat(getComputedStyle(document.documentElement).fontSize);
      const chipBox = box(chip);
      return {
        rem,
        cell: box(cell),
        tab: box(cell?.querySelector('[role="tab"]')),
        chip: chipBox,
        closer: box(cell?.querySelector(".closer")),
        // The counts the chip shows, which are the ones on its one line.
        shown: [...(counts?.querySelectorAll<HTMLElement>(".count") ?? [])]
          .filter((count) => {
            const at = count.getBoundingClientRect();
            return chipBox !== null && at.top < chipBox.bottom - 1 && at.right <= chipBox.right + 1;
          })
          .map((count) => count.getAttribute("data-count")),
        all: [...(counts?.querySelectorAll<HTMLElement>(".count") ?? [])].map((count) =>
          count.getAttribute("data-count"),
        ),
        nameCut: label ? label.scrollWidth > label.clientWidth + 1 : false,
        said: counts?.getAttribute("aria-label") ?? null,
      };
    },
    STRIP,
    name,
  );
}

/** How many task menus are open. */
async function menus(): Promise<number> {
  return browser.execute(() => document.querySelectorAll('[role="menu"].tasks-menu').length);
}

/**
 * Takes the pointer along the strip, onto each thing in it in turn and off the end: every
 * tab, every chip's hand and counts, every close. `dwell` is how long it stays on each, in
 * milliseconds. Answers how many things it went over, and how many of them were chips.
 */
async function cross(dwell: number): Promise<{ over: number; chips: number }> {
  return browser.execute(
    async (strip: string, stay: number) => {
      const stops = [
        ...document.querySelectorAll<HTMLElement>(
          `${strip} [role="tab"], ${strip} .tab-tasks button, ${strip} .closer`,
        ),
      ].sort((a, b) => a.getBoundingClientRect().left - b.getBoundingClientRect().left);
      const wait = (ms: number) => new Promise((done) => setTimeout(done, ms));
      const send = (on: Element, type: string, related: Element | null) => {
        const at = on.getBoundingClientRect();
        on.dispatchEvent(
          new PointerEvent(type, {
            bubbles: true,
            cancelable: true,
            pointerId: 1,
            pointerType: "mouse",
            isPrimary: true,
            clientX: at.left + at.width / 2,
            clientY: at.top + at.height / 2,
            relatedTarget: related,
          }),
        );
      };
      let last: Element | null = null;
      for (const stop of stops) {
        if (last === null) send(stop, "pointerover", null);
        else send(last, "pointerout", stop);
        // A move at its own point on each thing: the pointer is moving, which is what would
        // start a rest on a chip if it then stayed.
        send(stop, "pointermove", null);
        last = stop;
        await wait(stay);
      }
      if (last !== null) send(last, "pointerout", null);
      return {
        over: stops.length,
        chips: stops.filter((stop) => stop.classList.contains("tab-tasks-counts")).length,
      };
    },
    STRIP,
    dwell,
  );
}

/** Puts the pointer on the counts of the chip on the tab called `name`, or takes it off. */
async function pointer(name: string, what: "on" | "off"): Promise<boolean> {
  return browser.execute(
    (strip: string, called: string, how: string) => {
      const cell = [...document.querySelectorAll<HTMLElement>(`${strip} > .tab`)].find(
        (one) => one.querySelector(".tab-name")?.textContent === called,
      );
      const counts = cell?.querySelector<HTMLElement>(".tab-tasks-counts");
      if (!counts) return false;
      const at = counts.getBoundingClientRect();
      const send = (type: string, dx = 0) =>
        counts.dispatchEvent(
          new PointerEvent(type, {
            bubbles: true,
            cancelable: true,
            pointerId: 1,
            pointerType: "mouse",
            isPrimary: true,
            clientX: at.left + at.width / 2 + dx,
            clientY: at.top + at.height / 2,
            relatedTarget: null,
          }),
        );
      if (how === "on") {
        // Onto it, and then a move to another point: a rest is counted from a move the
        // person made, never from the pointer merely being found there.
        send("pointerover");
        send("pointermove", 2);
      } else send("pointerout");
      return true;
    },
    STRIP,
    name,
    what,
  );
}

function check(
  what: string,
  value: number | boolean,
  is: "atLeast" | "atMost" | "is",
  other: number | boolean,
) {
  const holds = is === "is" ? value === other : is === "atLeast" ? value >= other : value <= other;
  if (!holds) throw new Error(`${what}: ${String(value)} is not ${is} ${String(other)}`);
}

describe("the chip a session's tab wears for its tasks", () => {
  /** The tabs that were already there, so only this file's own are ended again. */
  let wereAlreadyOpen: string[] = [];
  let was = { width: 1280, height: 800 };
  /** This file's own tabs, in the order they were opened, and the first one's chat. */
  let mine: string[] = [];
  let asker = -1;

  async function windowIs(width: number, height: number) {
    await browser.setWindowSize(width, height);
    let last = -1;
    await browser.waitUntil(
      async () => {
        const now = await browser.execute(() => window.innerWidth);
        const settled = now === last;
        last = now;
        return settled;
      },
      { timeout: 10_000, interval: 200, timeoutMsg: "the window never settled on a width" },
    );
  }

  async function untilChip(name: string, worn: boolean) {
    await browser.waitUntil(async () => ((await cells())[name]?.chip ?? false) === worn, {
      timeout: 10_000,
      interval: 100,
      timeoutMsg: `${name}'s tab ${worn ? "never wore" : "kept"} a chip`,
    });
  }

  before(async () => {
    // **It starts from a strip of its own.** The strip draws what fits and the tab in front,
    // so what earlier files left open decides which of this file's tabs are drawn, and how
    // wide each is: measured in CI, eleven tabs left behind, and this file's three not drawn.
    // Ended the way other files end what they leave (`endEveryChat`).
    await endEveryChat();
    wereAlreadyOpen = await tabNames();
    was = await browser.getWindowSize();
    await windowIs(1280, 800);
    // **Each tab is waited for by itself, and by its name.** The pane in front already shows
    // the harness's word from the tab before, so "a pane shows it" is true at once: without
    // a wait of its own the next tab was pressed for while this one was still starting. And
    // not by a count of the strip: one app serves the whole run, and a tab the spec before
    // this one was still closing goes while this one opens, so "one more than there were"
    // never comes true (CI, twice). A tab is new when the strip draws a name it had not.
    mine = [];
    for (let opened = 0; opened < 3; opened++) {
      const known = new Set([...wereAlreadyOpen, ...(await tabNames()), ...mine]);
      await pressAndStart("New tab");
      let saw = "nothing was read";
      let came: string | undefined;
      await browser
        .waitUntil(
          async () => {
            const now = await strip();
            saw = JSON.stringify(now);
            came = now.tabs.find((name) => name !== "" && !known.has(name));
            return came !== undefined;
          },
          {
            timeout: 30_000,
            interval: 250,
            timeoutMsg: `tab ${opened + 1} of 3 never came onto the strip`,
          },
        )
        .catch((err: unknown) => {
          // What the strip held, so a red run says what it saw.
          throw new Error(
            `${String(err)}; before the press: ${JSON.stringify([...known])}; last seen: ${saw}`,
          );
        });
      await untilShows(READY);
      // Its name is read once it has stopped changing: a tab is named before its chat has
      // told the window what it is called.
      let last = "";
      await browser
        .waitUntil(
          async () => {
            const names = (await tabNames()).filter((name) => name !== "" && !known.has(name));
            const settled = names.length === 1 && names[0] === last;
            last = names[0] ?? "";
            return settled;
          },
          {
            timeout: 10_000,
            interval: 400,
            timeoutMsg: `tab ${opened + 1} of 3 never settled on a name`,
          },
        )
        .catch(async (err: unknown) => {
          throw new Error(`${String(err)}; the strip: ${JSON.stringify(await strip())}`);
        });
      mine.push(last);
    }
    // All three are drawn: a strip that had hidden one would be measuring something else.
    const drawn = await strip();
    if (!mine.every((name) => drawn.tabs.includes(name)))
      throw new Error(
        `the strip does not draw this file's three tabs ${JSON.stringify(mine)}: ${JSON.stringify(drawn)}`,
      );
    await select(mine[0]);
    await browser.waitUntil(async () => (await sessionInFront()) > 0, { timeout: 10_000 });
    asker = await sessionInFront();
  });

  afterEach(async () => {
    await pointer(mine[0], "off");
    await pretend(asker, []);
    await windowIs(1280, 800);
  });

  // One app process serves the whole run: nothing pretended is left, the window goes back to
  // the size it had, and every chat this file opened is ended.
  // **Every step is tried whatever became of the one before it**: a case that failed half
  // way must not leave its chats for the specs that share this app.
  after(async () => {
    const tried = async (step: () => Promise<unknown>) => {
      try {
        await step();
      } catch {
        // The next step still runs; the wait below says what was left.
      }
    };
    await tried(() => pretend(asker, []));
    await tried(() => browser.keys("Escape"));
    await tried(() => browser.setWindowSize(was.width, was.height));
    for (let round = 0; round < 5; round++) {
      const left = (await tabNames()).filter((tab) => !wereAlreadyOpen.includes(tab));
      if (left.length === 0) break;
      for (const name of left) await tried(() => endChat(`End chat ${name}`));
      await browser.pause(500);
    }
    await browser.waitUntil(
      async () => (await tabNames()).every((tab) => wereAlreadyOpen.includes(tab)),
      { timeout: 20_000, timeoutMsg: "the tab-chip spec left a chat open behind it" },
    );
  });

  it("leaves every tab the width it was, the one with tasks and the ones without", async () => {
    expect(mine.length).toBeGreaterThanOrEqual(3);
    // As the strip was before any tab had a task: what it is without this change.
    const before = await cells();
    for (const name of mine)
      check(`${name} wore a chip with no tasks`, before[name].chip, "is", false);

    await pretend(asker, ["working", "working", "failed", "done", "done", "done"]);
    await untilChip(mine[0], true);
    const after = await cells();

    for (const [name, { box }] of Object.entries(before)) {
      const now = after[name];
      check(`${name} is gone from the strip`, now !== undefined, "is", true);
      check(`${name} changed width`, Math.abs(now.box.width - box.width), "atMost", 0.5);
      check(`${name} moved`, Math.abs(now.box.left - box.left), "atMost", 0.5);
    }
    // Only the session that has tasks wears one.
    for (const name of mine.slice(1)) check(`${name} wears a chip`, after[name].chip, "is", false);

    const seen = await measured(mine[0]);
    if (seen.shown.length !== 3)
      console.log(`tab-chip.e2e: the chip with room was ${JSON.stringify(seen)}`);
    expect(seen.said).toContain("2 working, 1 failed, 3 done");
    // With room to spare the chip shows every count, and sits inside its tab's own cell,
    // between the tab's button and its close.
    expect(seen.shown).toEqual(["working", "failed", "done"]);
    const [cell, tab, chip, closer] = [seen.cell, seen.tab, seen.chip, seen.closer] as Box[];
    check("the chip starts over the tab's button", chip.left, "atLeast", tab.right - 1);
    check("the chip runs under the close", chip.right, "atMost", closer.left + 1);
    check("the close left its cell", closer.right, "atMost", cell.right + 1);
  });

  it("opens no menu for a pointer that crosses the strip", async () => {
    await pretend(asker, ["working", "done"]);
    await untilChip(mine[0], true);

    // Quickly, as a pointer on its way somewhere else, and slowly, at a third of the rest on
    // each thing: neither is a rest on the chip.
    for (const dwell of [16, 110]) {
      const went = await cross(dwell);
      check("the pointer met no chip on its way", went.chips, "atLeast", 1);
      check("the pointer crossed nothing", went.over, "atLeast", mine.length * 2);
      // Long past the rest, in case anything was still counting.
      await browser.pause(900);
      expect(await menus()).toBe(0);
    }
  });

  it("opens no menu for a chip that appears under a pointer that is not moving", async () => {
    // The pointer is parked where the chip will be drawn, and then the session gets tasks:
    // the engine finds the pointer on the new chip, and nobody's hand moved.
    await pretend(asker, ["working"]);
    await untilChip(mine[0], true);
    const parked = await browser.execute((strip: string) => {
      const counts = document.querySelector<HTMLElement>(`${strip} .tab-tasks-counts`);
      if (!counts) return false;
      const at = counts.getBoundingClientRect();
      const here = {
        bubbles: true,
        cancelable: true,
        pointerId: 1,
        pointerType: "mouse",
        isPrimary: true,
        clientX: at.left + at.width / 2,
        clientY: at.top + at.height / 2,
      };
      counts.dispatchEvent(new PointerEvent("pointerover", { ...here, relatedTarget: null }));
      // The same point again, as an engine says "still here" after a layout.
      counts.dispatchEvent(new PointerEvent("pointermove", here));
      counts.dispatchEvent(new PointerEvent("pointermove", here));
      return true;
    }, STRIP);
    expect(parked).toBe(true);

    await browser.pause(1_200);
    expect(await menus()).toBe(0);
  });

  it("opens the menu once the pointer has rested on the chip, and closes it once it has gone", async () => {
    await pretend(asker, ["working", "failed", "done"]);
    await untilChip(mine[0], true);

    expect(await pointer(mine[0], "on")).toBe(true);
    // Not at once: a third of the way into the rest there is still no menu.
    await browser.pause(110);
    expect(await menus()).toBe(0);
    await browser.waitUntil(async () => (await menus()) === 1, {
      timeout: 5_000,
      interval: 50,
      timeoutMsg: "resting on the chip opened no menu",
    });

    const seen = await browser.execute(() => {
      const menu = document.querySelector<HTMLElement>('[role="menu"].tasks-menu');
      const chip = document.querySelector<HTMLElement>(".tab-tasks-counts");
      if (!menu || !chip) return null;
      const at = menu.getBoundingClientRect();
      return {
        rows: [...menu.querySelectorAll('[role="menuitem"] .name')].map((row) => row.textContent),
        // A state word that is cut is wider inside than its box.
        cut: [...menu.querySelectorAll<HTMLElement>(".shown-state .word")]
          .filter((word) => word.scrollWidth > word.clientWidth + 1)
          .map((word) => word.textContent),
        below: at.top >= chip.getBoundingClientRect().bottom - 1,
        inside: at.left >= 0 && at.right <= window.innerWidth && at.bottom <= window.innerHeight,
        // It took no keyboard: the pointer opened it, and whoever was typing still is.
        keyboardInIt: menu.contains(document.activeElement),
      };
    });
    expect(seen).not.toBeNull();
    // The session's own chat first, then the tasks; the finished one is in the fold.
    expect(seen?.rows.slice(0, 1)).toEqual([mine[0]]);
    expect(seen?.rows).toContain("pretended task 1");
    expect(seen?.rows).toContain("pretended task 2");
    expect(seen?.rows).toContain("Finished (1)");
    expect(seen?.cut).toEqual([]);
    expect(seen?.below).toBe(true);
    expect(seen?.inside).toBe(true);
    expect(seen?.keyboardInIt).toBe(false);

    await pointer(mine[0], "off");
    await browser.waitUntil(async () => (await menus()) === 0, {
      timeout: 5_000,
      interval: 50,
      timeoutMsg: "the menu stayed after the pointer had left the chip",
    });
  });

  it("keeps a rest-opened menu while the pointer goes onto End a task and into its rows", async () => {
    // The ways to end a task are in a menu of their own, drawn in a portal beside the first
    // (#1488 in #1487's menu). A pointer going from the menu into it has not left the menu:
    // the first must stay, and the second must be drawn.
    await pretend(asker, ["working", "working"]);
    await untilChip(mine[0], true);
    expect(await pointer(mine[0], "on")).toBe(true);
    await browser.waitUntil(async () => (await menus()) === 1, {
      timeout: 5_000,
      interval: 50,
      timeoutMsg: "resting on the chip opened no menu",
    });

    // Off the chip and onto the line that opens the ways to end a task, as a hand moves.
    const onTheLine = await browser.execute(() => {
      const counts = document.querySelector<HTMLElement>(".tab-tasks-counts");
      const line = document.querySelector<HTMLElement>(".tasks-menu .tasks-menu-end");
      if (!counts || !line) return false;
      const at = line.getBoundingClientRect();
      const here = {
        bubbles: true,
        cancelable: true,
        pointerId: 1,
        pointerType: "mouse",
        isPrimary: true,
        clientX: at.left + at.width / 2,
        clientY: at.top + at.height / 2,
      };
      counts.dispatchEvent(new PointerEvent("pointerout", { ...here, relatedTarget: line }));
      line.dispatchEvent(new PointerEvent("pointerover", { ...here, relatedTarget: counts }));
      line.dispatchEvent(new PointerEvent("pointerenter", { ...here, bubbles: false }));
      line.dispatchEvent(new PointerEvent("pointermove", here));
      line.dispatchEvent(new PointerEvent("pointermove", { ...here, clientX: here.clientX + 2 }));
      return true;
    });
    expect(onTheLine).toBe(true);
    await browser
      .waitUntil(
        async () =>
          browser.execute(() => document.querySelector('[role="menu"].tasks-menu-ends') !== null),
        { timeout: 5_000, interval: 50, timeoutMsg: "the ways to end a task never opened" },
      )
      .catch(async (err: unknown) => {
        const menu = await browser.execute(() => ({
          menus: document.querySelectorAll('[role="menu"]').length,
          rows: [...document.querySelectorAll('.tasks-menu [role="menuitem"]')].map(
            (row) => `${row.className}: ${row.textContent ?? ""}`,
          ),
          line: document.querySelector(".tasks-menu-end")?.outerHTML.slice(0, 300) ?? null,
        }));
        throw new Error(`${String(err)}; the menu: ${JSON.stringify(menu)}`);
      });

    // Into its rows, and well past the time a menu left by the pointer would have closed.
    const inside = await browser.execute(() => {
      const from = document.querySelector<HTMLElement>(".tasks-menu .tasks-menu-end");
      const row = document.querySelector<HTMLElement>('.tasks-menu-ends [role="menuitem"]');
      if (!from || !row) return false;
      const at = row.getBoundingClientRect();
      const here = {
        bubbles: true,
        cancelable: true,
        pointerId: 1,
        pointerType: "mouse",
        isPrimary: true,
        clientX: at.left + at.width / 2,
        clientY: at.top + at.height / 2,
      };
      from.dispatchEvent(new PointerEvent("pointerout", { ...here, relatedTarget: row }));
      row.dispatchEvent(new PointerEvent("pointerover", { ...here, relatedTarget: from }));
      row.dispatchEvent(new PointerEvent("pointermove", here));
      return true;
    });
    expect(inside).toBe(true);
    await browser.pause(900);
    const still = await browser.execute(() => ({
      first: document.querySelector('[role="menu"].tasks-menu:not(.tasks-menu-ends)') !== null,
      ways: [...document.querySelectorAll('.tasks-menu-ends [role="menuitem"] .name')].map(
        (row) => row.textContent ?? "",
      ),
      // What stands just before the last row: the line Stop all tasks sits under (#1498).
      beforeLast:
        document
          .querySelector(".tasks-menu-ends .tasks-menu-stop-all")
          ?.previousElementSibling?.getAttribute("role") ?? null,
    }));
    expect(still.first).toBe(true);
    // Each open task's two ways, by the catalogue's own titles, and nothing was ended.
    const each = still.ways.slice(0, -1);
    expect(each).toHaveLength(4);
    expect(each.every((title) => /^(Stop and get its report|Close now): task /.test(title))).toBe(
      true,
    );
    // Then Stop all tasks, last and under a line of its own (#1498, D-1498-8).
    expect(still.ways.at(-1)).toBe("Stop all tasks");
    expect(still.beforeLast).toBe("separator");
    await browser.keys("Escape");
    await browser.keys("Escape");
  });

  it("gives up the name before a count on a crowded strip, and overflows nothing", async () => {
    await pretend(asker, ["working", "working", "failed", "done", "done", "done"]);
    await untilChip(mine[0], true);

    // Narrower and narrower: the tabs go down to the least a tab is drawn at.
    for (const width of [900, 700, 560]) {
      await windowIs(width, 800);
      // The tab in front is always drawn, whatever the strip hides.
      await select(mine[0]);
      await untilChip(mine[0], true);
      const seen = await measured(mine[0]);
      const [cell, tab, chip, closer] = [seen.cell, seen.tab, seen.chip, seen.closer] as Box[];
      const at = `at ${width}px`;
      // A red run says the cell it measured.
      console.log(`tab-chip.e2e: ${at} the tab with a chip was ${JSON.stringify(seen)}`);

      // Its share of the strip is the share of every other tab drawn.
      for (const [name, other] of Object.entries(await cells()))
        check(
          `${at}: ${name} is not as wide as the tab with a chip`,
          Math.abs(other.box.width - cell.width),
          "atMost",
          1,
        );
      // Nothing in the cell is on top of anything else in it, or outside it.
      check(`${at}: the chip is over the tab's button`, chip.left, "atLeast", tab.right - 1);
      check(`${at}: the chip is under the close`, chip.right, "atMost", closer.left + 1);
      check(`${at}: the close left its cell`, closer.right, "atMost", cell.right + 1);
      check(`${at}: the tab's button starts before its cell`, tab.left, "atLeast", cell.left - 1);
      // The count of what is still working is the last to go, and it never goes.
      check(`${at}: the chip shows no count`, seen.shown.length, "atLeast", 1);
      expect(seen.shown[0]).toBe("working");
      // **The rule**: a count is given up only once the name has given up all it can. While
      // the tab's button is wider than its floor, every count is drawn.
      if (seen.shown.length < seen.all.length) {
        check(
          `${at}: a count went while the name had room to give`,
          tab.width,
          "atMost",
          2 * seen.rem + 1,
        );
        check(`${at}: a count went and the name is whole`, seen.nameCut, "is", true);
      }
      // And the name is never squeezed out: the button keeps its floor.
      check(`${at}: the tab's button is under its floor`, tab.width, "atLeast", 2 * seen.rem - 1);
    }
  });
});
