import { browser, expect, $$ } from "@wdio/globals";
import { READY } from "../harness.js";
import { endChat, pressAndStart } from "../opening.js";

/**
 * **A pane's breadcrumb fits its pane's one top line, measured** (#1486).
 *
 * While a tab shows a task, the pane's existing top line says which chat is on screen:
 * `steward 4 › talk · working`. The operator's ruling was a line and no bar (V100-31, V100-34):
 * the pane gains no row. And a narrow pane is where that is hard: the path is long, the pane is
 * a few hundred pixels, and the state word is the part that must never be cut.
 *
 * All of that is layout, and jsdom lays nothing out, so it is measured here: in the real
 * engine, against the built stylesheet, in real panes of a real split.
 *
 * **What is drawn is the shape, not the cause.** A task needs a persona chat and a core that
 * dispatched it, which this suite's fake harness has no way to make. So a breadcrumb is drawn
 * into the pane's own top line (`.pane-chips`, which `PaneFrame` draws for every pane) with the
 * elements and classes `PaneCrumbs` draws. `PaneCrumbs.test.tsx` holds the component to that
 * shape and `TaskInTab.window.test.tsx` to where it is drawn; this file holds the stylesheet to
 * what it must look like. A class renamed in one place and not the other fails one of them.
 */

type Box = {
  left: number;
  right: number;
  top: number;
  bottom: number;
  width: number;
  height: number;
};

/** A task of a task, with names a person would give: far longer than a narrow pane's line. */
const LONG_PATH = [
  "release verification steward 14",
  "read-only check of the production release notes",
  "compare the changelog against the tagged commits",
  "live check talk",
];
/** The longest state there is (`shownState`). */
const LONGEST_STATE = "ended without a report";

async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [
      ...(document
        .querySelector('[role="tablist"][aria-label="Tabs"]')
        ?.querySelectorAll('[role="tab"]') ?? []),
    ].map((tab) => tab.querySelector(".tab-name")?.textContent ?? ""),
  );
}

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

/**
 * Draws a breadcrumb first in pane `pane`'s top line, as `PaneCrumbs` draws one: the path (a
 * button for each chat before the last, a name for the last), the workspace where `elsewhere`,
 * and the state as its mark and its word. Answers whether the pane had a line to draw it in.
 */
async function draw(
  pane: number,
  crumb: { path: string[]; state: string; elsewhere?: string },
): Promise<boolean> {
  return browser.execute(
    (at: number, path: string[], state: string, elsewhere: string | null) => {
      const frames = [...document.querySelectorAll(".pane-frame")].filter((frame) =>
        frame.querySelector('[data-testid="pane"]'),
      );
      const line = frames[at]?.querySelector(".pane-corner.at-start > .pane-chips");
      if (!line) return false;
      const el = (tag: string, cls: string, text?: string) => {
        const made = document.createElement(tag);
        made.className = cls;
        if (text !== undefined) made.textContent = text;
        return made;
      };
      const crumbs = el("nav", "pane-crumbs");
      crumbs.dataset.drawn = "pane-crumbs.e2e";
      const way = el("span", "crumb-path");
      path.forEach((name, index) => {
        const last = index === path.length - 1;
        if (last) way.append(el("span", "crumb shown", name));
        else {
          const button = el("button", index === 0 ? "crumb own" : "crumb between", name);
          button.setAttribute("type", "button");
          button.tabIndex = 0;
          way.append(button, el("span", "crumb-sep", " › "));
        }
      });
      crumbs.append(way);
      if (elsewhere !== null) crumbs.append(el("span", "crumb-where", ` in ${elsewhere}`));
      crumbs.append(el("span", "crumb-sep", " · "));
      const shown = el("span", "shown-state");
      const shape = el("span", "shape");
      // The mark's own size, as its icon has it (`App.css`, `.shown-state .shape svg`).
      shape.append(document.createElementNS("http://www.w3.org/2000/svg", "svg"));
      shown.append(shape, el("span", "word", state));
      crumbs.append(shown);
      line.prepend(crumbs);
      return true;
    },
    pane,
    crumb.path,
    crumb.state,
    crumb.elsewhere ?? null,
  );
}

/**
 * Draws the two ways to end the task after pane `pane`'s breadcrumb, as `TaskEnds` draws them
 * (#1488): a group of two word buttons. Answers whether there was a breadcrumb to draw after.
 */
async function drawEnds(pane: number): Promise<boolean> {
  return browser.execute((at: number) => {
    const frames = [...document.querySelectorAll(".pane-frame")].filter((frame) =>
      frame.querySelector('[data-testid="pane"]'),
    );
    const crumbs = frames[at]?.querySelector('[data-drawn="pane-crumbs.e2e"]');
    if (!crumbs) return false;
    const group = document.createElement("span");
    group.className = "pane-task-ends";
    group.dataset.drawn = "pane-crumbs.e2e";
    for (const words of ["Stop", "Close now"]) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "task-end";
      button.tabIndex = 0;
      button.textContent = words;
      group.append(button);
    }
    crumbs.after(group);
    return true;
  }, pane);
}

/** The group of the two ways to end the task in pane `pane`, and each button as it is drawn. */
async function measuredEnds(pane: number) {
  return browser.execute((at: number) => {
    const box = (el: Element) => {
      const { left, right, top, bottom, width, height } = el.getBoundingClientRect();
      return { left, right, top, bottom, width, height };
    };
    const frames = [...document.querySelectorAll(".pane-frame")].filter((frame) =>
      frame.querySelector('[data-testid="pane"]'),
    );
    const group = frames[at]?.querySelector(".pane-task-ends");
    if (!group) return null;
    return {
      group: box(group),
      buttons: [...group.querySelectorAll<HTMLElement>(".task-end")].map((button) => ({
        words: button.textContent ?? "",
        box: box(button),
        cut: button.scrollWidth > button.clientWidth + 1,
      })),
    };
  }, pane);
}

/** Takes away everything this file drew, and puts back every control it took away. */
async function erase(): Promise<void> {
  await browser.execute(() => {
    for (const one of document.querySelectorAll('[data-drawn="pane-crumbs.e2e"]')) one.remove();
    for (const one of document.querySelectorAll<HTMLElement>('[data-taken="pane-crumbs.e2e"]')) {
      one.style.display = "";
      delete one.dataset.taken;
    }
  });
}

/**
 * Makes pane `pane`'s own controls what a pane showing a task draws: the two splits, and no
 * close (`PaneDoing`, `task`). Then waits until the pane has said the width they now have
 * (`--pane-controls`), which is what its top line reserves. Answers how many are drawn.
 */
async function asATaskPane(pane: number): Promise<number> {
  const drawn = await browser.execute((at: number) => {
    const frames = [...document.querySelectorAll(".pane-frame")].filter((frame) =>
      frame.querySelector('[data-testid="pane"]'),
    );
    const controls = frames[at]?.querySelector(".pane-corner.at-end > .pane-doing");
    const close = [...(controls?.querySelectorAll<HTMLElement>("button") ?? [])].find((button) =>
      (button.getAttribute("aria-label") ?? "").startsWith("End this pane"),
    );
    if (close) {
      close.style.display = "none";
      close.dataset.taken = "pane-crumbs.e2e";
    }
    return [...(controls?.querySelectorAll<HTMLElement>("button") ?? [])].filter(
      (button) => button.style.display !== "none",
    ).length;
  }, pane);
  await untilReserved(pane);
  return drawn;
}

/** Waits until pane `pane`'s frame says the width its controls have now. */
async function untilReserved(pane: number): Promise<void> {
  await browser.waitUntil(
    async () =>
      browser.execute((at: number) => {
        const frames = [...document.querySelectorAll<HTMLElement>(".pane-frame")].filter((frame) =>
          frame.querySelector('[data-testid="pane"]'),
        );
        const frame = frames[at];
        const controls = frame?.querySelector<HTMLElement>(".pane-corner.at-end > .pane-doing");
        if (!frame || !controls) return false;
        const said = Number.parseFloat(frame.style.getPropertyValue("--pane-controls"));
        return Math.abs(said - controls.offsetWidth) <= 1;
      }, pane),
    { timeout: 10_000, interval: 100, timeoutMsg: `pane ${pane} never said its controls' width` },
  );
}

/** Every box this file needs of pane `pane`, its top line and the breadcrumb drawn in it. */
async function measured(pane: number) {
  return browser.execute((at: number) => {
    const box = (el: Element | null | undefined) => {
      if (!el) return null;
      const { left, right, top, bottom, width, height } = el.getBoundingClientRect();
      return { left, right, top, bottom, width, height };
    };
    /** Whether an element's text is cut: more of it than its box shows. */
    const cut = (el: Element | null | undefined) =>
      el instanceof HTMLElement ? el.scrollWidth > el.clientWidth + 1 : false;
    const frames = [...document.querySelectorAll(".pane-frame")].filter((frame) =>
      frame.querySelector('[data-testid="pane"]'),
    );
    const frame = frames[at];
    const line = frame?.querySelector(".pane-corner.at-start > .pane-chips");
    const crumbs = frame?.querySelector('[data-drawn="pane-crumbs.e2e"]');
    const word = crumbs?.querySelector<HTMLElement>(".shown-state .word");
    return {
      rem: Number.parseFloat(getComputedStyle(document.documentElement).fontSize),
      frame: box(frame),
      terminal: box(frame?.querySelector('[data-testid="pane"]')),
      corner: box(frame?.querySelector(".pane-corner.at-start")),
      line: box(line),
      // The line's other items, which were there before the breadcrumb: the gauge, the harness.
      others: [...(line?.children ?? [])]
        .filter((one) => one !== crumbs)
        .map((one) => ({ what: one.className, box: box(one) })),
      controls: box(frame?.querySelector(".pane-corner.at-end > .pane-doing")),
      // The width the pane says its controls have, which its top line reserves.
      reserved: Number.parseFloat(
        (frame as HTMLElement | undefined)?.style.getPropertyValue("--pane-controls") ?? "",
      ),
      crumbs: box(crumbs),
      names: [...(crumbs?.querySelectorAll(".crumb") ?? [])].map((name) => ({
        text: name.textContent ?? "",
        between: name.classList.contains("between"),
        box: box(name),
        cut: cut(name),
      })),
      where: box(crumbs?.querySelector(".crumb-where")),
      state: box(crumbs?.querySelector(".shown-state")),
      word: box(word),
      wordCut: cut(word),
      // What the word needs to be read whole, on one line.
      wordNeeds: word ? word.scrollWidth : 0,
    };
  }, pane);
}

/** One measured claim, failing with what was measured (as `pane-notices.e2e.ts` has it). */
function check(
  what: string,
  value: number | boolean,
  is: "atLeast" | "atMost" | "is",
  other: number | boolean,
) {
  const holds = is === "is" ? value === other : is === "atLeast" ? value >= other : value <= other;
  if (!holds) throw new Error(`${what}: ${String(value)} is not ${is} ${String(other)}`);
}

/** `inner` is inside `outer` on all four sides, to within a pixel of rounding. */
function inside(inner: Box | null, outer: Box | null, what: string) {
  check(`${what}: nothing was drawn`, inner !== null, "is", true);
  check(`${what}: nothing to be inside`, outer !== null, "is", true);
  const [a, b] = [inner as Box, outer as Box];
  check(`${what} starts left of it`, a.left, "atLeast", b.left - 1);
  check(`${what} runs past its right edge`, a.right, "atMost", b.right + 1);
  check(`${what} starts above it`, a.top, "atLeast", b.top - 1);
  check(`${what} runs past its bottom edge`, a.bottom, "atMost", b.bottom + 1);
}

/** Whether two boxes share any area. */
const overlap = (a: Box, b: Box) =>
  a.left < b.right - 1 && b.left < a.right - 1 && a.top < b.bottom - 1 && b.top < a.bottom - 1;

/** How far anything is scrolled sideways, from the pane's frame up to the page: all 0 when
 *  nothing overflowed. */
async function scrolledSideways(pane: number): Promise<number[]> {
  return browser.execute((at: number) => {
    const frames = [...document.querySelectorAll(".pane-frame")].filter((frame) =>
      frame.querySelector('[data-testid="pane"]'),
    );
    const all: number[] = [];
    for (let el: Element | null = frames[at] ?? null; el; el = el.parentElement)
      all.push(Math.round(el.scrollLeft));
    all.push(Math.round(window.scrollX));
    return all;
  }, pane);
}

describe("a pane's breadcrumb, while its tab shows a task", () => {
  /** The tabs that were already there, so only this file's own are ended again. */
  let wereAlreadyOpen: string[] = [];
  let was = { width: 1280, height: 800 };

  before(async () => {
    wereAlreadyOpen = await tabNames();
    was = await browser.getWindowSize();
    await pressAndStart("New tab");
    await untilShows(0, READY);
    await pressAndStart("Split right");
    await untilShows(1, READY);
  });

  afterEach(erase);

  // One app process serves the whole run: the window goes back to the size it had, and every
  // chat this file opened is ended (as `pane-notices.e2e.ts` ends its own).
  after(async () => {
    await browser.setWindowSize(was.width, was.height);
    for (let round = 0; round < 5; round++) {
      const mine = (await tabNames()).filter((tab) => !wereAlreadyOpen.includes(tab));
      if (mine.length === 0) break;
      for (const name of mine) await endChat(`End chat ${name}`);
      await browser.pause(500);
    }
    await browser.waitUntil(
      async () => (await tabNames()).every((tab) => wereAlreadyOpen.includes(tab)),
      { timeout: 20_000, timeoutMsg: "the pane-crumbs spec left a chat open behind it" },
    );
  });

  /** Asks for a window of this size and waits until the page has stopped changing width. */
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
      { timeout: 20_000, interval: 250, timeoutMsg: "the window never stopped resizing" },
    );
  }

  it("sits in the pane's existing top line, and the pane gains no row", async () => {
    await windowIs(1280, 800);
    const before = await measured(0);
    expect(await draw(0, { path: ["steward 4", "talk"], state: "working" })).toBe(true);
    const seen = await measured(0);

    // In the line, level with what was already in it: the gauge is beside it, not under it.
    inside(seen.crumbs, seen.line, "the breadcrumb in the top line");
    for (const other of seen.others) {
      const [a, b] = [seen.crumbs as Box, other.box as Box];
      check(`${other.what} is on another row than the breadcrumb`, a.top, "atMost", b.bottom - 1);
      check(`${other.what} is on another row than the breadcrumb`, b.top, "atMost", a.bottom - 1);
      check(`${other.what} is under or over the breadcrumb`, overlap(a, b), "is", false);
    }
    // No row was added: the line is one line of its text tall, no taller than it was by more
    // than the mark beside the word, and the terminal has not moved or shrunk.
    const line = seen.line as Box;
    check("the top line is taller than two lines of its text", line.height, "atMost", seen.rem * 2);
    if (before.others.length > 0)
      check("the top line grew a row", line.height, "atMost", (before.line as Box).height + 4);
    expect(seen.terminal).toEqual(before.terminal);
    expect(seen.frame).toEqual(before.frame);
    // With room to spare nothing is cut: every name and the state are whole.
    expect(seen.names.map((name) => name.cut)).toEqual([false, false]);
    expect(seen.wordCut).toBe(false);
  });

  // Each narrow pane twice: with the controls a pane showing a task draws (the two splits),
  // and with the three every other pane draws. The line reserves what is there, measured, so
  // neither is a number this file or the stylesheet has to know.
  for (const [pane, task] of [
    [0, true],
    [1, true],
    [0, false],
    [1, false],
  ] as const) {
    it(`cuts the middle of a long path in narrow pane ${pane} with ${task ? "a task pane's two" : "three"} controls, keeps the state whole, and overflows nothing`, async () => {
      // 1024 px is the narrowest window purlis supports (ADR 0054), and with two panes side by
      // side each is a few hundred pixels.
      await windowIs(1024, 768);
      if (task) expect(await asATaskPane(pane)).toBe(2);
      else await untilReserved(pane);
      expect(
        await draw(pane, { path: LONG_PATH, state: LONGEST_STATE, elsewhere: "release-train" }),
      ).toBe(true);

      const seen = await measured(pane);
      const frame = seen.frame as Box;
      // The case this is about: a pane far narrower than the path it has to say.
      check("the pane is not a narrow one", frame.width, "atMost", 40 * seen.rem);

      // Nothing the line draws is outside the pane, and nothing was scrolled sideways by it.
      inside(seen.line, frame, "the top line in its pane");
      inside(seen.crumbs, frame, "the breadcrumb in its pane");
      for (const other of seen.others) inside(other.box, frame, `${other.what} in its pane`);
      expect(await scrolledSideways(pane)).toEqual((await scrolledSideways(pane)).map(() => 0));

      // The state is whole, on one line, inside the breadcrumb: it is what gives way last.
      expect(seen.wordCut).toBe(false);
      check(
        "the state word is narrower than it needs",
        (seen.word as Box).width,
        "atLeast",
        seen.wordNeeds - 1,
      );
      inside(seen.state, seen.crumbs, "the state in the breadcrumb");
      inside(seen.where, seen.crumbs, "the workspace in the breadcrumb");

      // The middle of the path gave way first: every name between the two ends is cut, and
      // each end still shows at least as much as a name between them does.
      const between = seen.names.filter((name) => name.between);
      const ends = seen.names.filter((name) => !name.between);
      expect(between).toHaveLength(2);
      expect(ends).toHaveLength(2);
      for (const name of between) check(`"${name.text}" is not cut`, name.cut, "is", true);
      const widest = Math.max(...between.map((name) => (name.box as Box).width));
      for (const name of ends) {
        check(
          `"${name.text}" is narrower than the middle`,
          (name.box as Box).width,
          "atLeast",
          widest - 1,
        );
        // Still a thing to read and to press: a letter or two and the ellipsis.
        check(`"${name.text}" is cut to nothing`, (name.box as Box).width, "atLeast", seen.rem);
      }

      // Nothing overlaps: not the line's other items, and not the pane's own controls in the
      // other corner, which appear on hover over what would be under them.
      for (const other of seen.others)
        check(
          `${other.what} is over the breadcrumb`,
          overlap(seen.crumbs as Box, other.box as Box),
          "is",
          false,
        );
      // The pane's controls are there, and the room the line keeps is their width.
      check("the pane draws no controls", seen.controls !== null, "is", true);
      if (seen.controls !== null) {
        check(
          "the line reserves less than the controls' width",
          seen.reserved,
          "atLeast",
          seen.controls.width - 1,
        );
        check(
          "the line ends under the controls",
          (seen.line as Box).right,
          "atMost",
          seen.controls.left + 1,
        );
        check(
          "the pane's controls are over the breadcrumb",
          overlap(seen.crumbs as Box, seen.controls),
          "is",
          false,
        );
        for (const other of seen.others)
          check(
            `the pane's controls are over ${other.what}`,
            overlap(other.box as Box, seen.controls),
            "is",
            false,
          );
      }
      // And the line is still one row.
      check("the top line wrapped", (seen.line as Box).height, "atMost", seen.rem * 2);
    });
  }
  // **The two ways to end the task are each drawn whole or not at all** (#1488): a narrow
  // line gives them up before anything of the path or the state, and never cuts one mid-word.
  for (const width of [1280, 900, 700, 560]) {
    it(`draws each way to end the task whole or not at all in a ${width}px window`, async () => {
      await windowIs(width, 800);
      await asATaskPane(0);
      expect(await draw(0, { path: LONG_PATH, state: LONGEST_STATE })).toBe(true);
      expect(await drawEnds(0)).toBe(true);
      const seen = await measured(0);
      const ends = await measuredEnds(0);
      check("the two ways were not drawn", ends !== null, "is", true);
      if (ends === null) return;

      // The group is one line of the top line, and added no row to it.
      check("the group is taller than a line", ends.group.height, "atMost", seen.rem * 2);
      check("the top line wrapped", (seen.line as Box).height, "atMost", seen.rem * 2);
      for (const button of ends.buttons) {
        // On the group's one line, or on a second line that is not drawn.
        const onTheLine = button.box.top < ends.group.bottom - 1;
        if (!onTheLine) continue;
        check(
          `"${button.words}" starts left of its group`,
          button.box.left,
          "atLeast",
          ends.group.left - 1,
        );
        check(
          `"${button.words}" is cut at its group's edge`,
          button.box.right,
          "atMost",
          ends.group.right + 1,
        );
        check(`"${button.words}" is cut mid-word`, button.cut, "is", false);
        check(
          `"${button.words}" runs past the top line`,
          button.box.right,
          "atMost",
          (seen.line as Box).right + 1,
        );
      }
      // They give way before the state word does: it is never cut to make room for them.
      if (ends.buttons.some((button) => button.box.top < ends.group.bottom - 1))
        check("the state word was cut to keep a button", seen.wordCut, "is", false);
    });
  }

  it("draws both ways to end the task where there is room", async () => {
    await windowIs(1280, 800);
    expect(await draw(0, { path: ["steward 4", "talk"], state: "working" })).toBe(true);
    expect(await drawEnds(0)).toBe(true);
    const ends = await measuredEnds(0);
    check("the two ways were not drawn", ends !== null, "is", true);
    if (ends === null) return;
    expect(
      ends.buttons.filter((button) => button.box.top < ends.group.bottom - 1 && !button.cut),
    ).toHaveLength(2);
  });
});
