import { browser, expect, $, $$ } from "@wdio/globals";
import { READY } from "../harness.js";
import { endChat, pressAndStart, pressOnly } from "../opening.js";

/**
 * **A Notice in a pane's corner fits its pane, measured** (#1481).
 *
 * The operator, from the dev build with two panes side by side: *"ui is broken for questions
 * modals"*. In a narrow pane a Notice's sentence was one word wide and fourteen lines tall, its
 * buttons each wrapped to five lines beside it, the block a button opened was drawn to the
 * right of the Notice and off the window, the Notice was drawn over an open dialog, and the
 * window itself was scrolled sideways by what overflowed it, so the band and the sidebar were
 * cut at the left.
 *
 * All of that is layout, and jsdom lays nothing out, so it is measured here: in the real
 * engine, against the built stylesheet, in real panes of a real split.
 *
 * **What is raised is the shape, not the cause.** A vault refusal and a held dispatch need a
 * persona chat and a core holding them, which this suite's fake harness has no way to make. So
 * a Notice is drawn into the pane's own stack (`.pane-notices`, which `PaneFrame` draws for
 * every pane) with the elements and classes `Notice at="pane"` draws, and the dialog with the
 * ones `AskPersona` draws. `Notice.pane.test.tsx` and `AskPersona.window.test.tsx` hold the
 * components to those shapes; this file holds the stylesheet to what they must look like. A
 * class renamed in one place and not the other fails one of the two.
 */

/** The sentence and the ways out of the Notice in the operator's screenshot. */
const SENTENCE =
  "This chat runs as steward, and vault devops is tagged for devops, so purlis did not open it. This chat has already asked devops: answer that above.";
const WAYS = ["Allow steward to use this vault", "Dispatch to devops…", "Keep blocked"];
/** The dispatch grant Notice's own sentence and its five answers (#1503), in the order drawn. */
const ASKS =
  "This chat runs as steward and wants to dispatch to devops. Nothing starts until you answer. Allowing it lets steward chats ask devops for anything devops can do, without asking you again.";
const FIVE_WAYS = [
  "Allow for this chat",
  "Allow for me on this machine",
  "Allow for everyone in this project",
  "Keep blocked",
  "Never for this pair",
];
/** A brief of 46 lines, some of them far longer than any pane is wide. */
const BRIEF = Array.from({ length: 46 }, (_, line) =>
  line % 5 === 0
    ? `${line} ${"an_unbroken_token_that_is_wider_than_a_pane_".repeat(8)}`
    : `${line} **Goal.** A READ-ONLY verification of prod release v2.48.0, with numbers.`,
).join("\n");

type Box = {
  left: number;
  right: number;
  top: number;
  bottom: number;
  width: number;
  height: number;
};

/** The tabs on the strip, left to right, as `pane-fill.e2e.ts` reads them. */
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
 * Draws a Notice into pane `pane`'s stack, as `Notice at="pane"` draws one: the box, the line
 * (a `status`) with the sentence and one button per way out, and, where `opened`, what a way
 * out opened under it. Answers whether the pane had a stack to draw it into.
 */
async function raise(
  pane: number,
  notice: { sentence: string; ways: string[]; opened?: string },
): Promise<boolean> {
  return browser.execute(
    (at: number, sentence: string, ways: string[], opened: string | null) => {
      const frames = [...document.querySelectorAll(".pane-frame")].filter((frame) =>
        frame.querySelector('[data-testid="pane"]'),
      );
      const stack = frames[at]?.querySelector(".pane-corner.at-start > .pane-notices");
      if (!stack) return false;
      const el = (tag: string, cls: string, text?: string) => {
        const made = document.createElement(tag);
        made.className = cls;
        if (text !== undefined) made.textContent = text;
        return made;
      };
      const box = el("div", opened === null ? "notice-pane-box" : "notice-pane-box notice-opened");
      box.dataset.raised = "pane-notices.e2e";
      const line = el("div", "notice notice-pane notice-trouble");
      line.setAttribute("role", "status");
      line.append(el("div", "notice-says", sentence));
      for (const way of ways) {
        const button = el("button", "notice-fix", way);
        button.setAttribute("type", "button");
        button.tabIndex = 0;
        line.append(button);
      }
      box.append(line);
      if (opened !== null) {
        const under = el("div", "notice-under notice-under-pane");
        const report = el("div", "block-report");
        report.append(
          el("p", "", "The brief, as the chat wrote it. purlis did not write it."),
          el("pre", "block-report-draft block-report-brief", opened),
          el("p", "", "The brief is 46 lines. Scroll its box to read all of it before you answer."),
        );
        under.append(report);
        box.append(under);
      }
      stack.append(box);
      return true;
    },
    pane,
    notice.sentence,
    notice.ways,
    notice.opened ?? null,
  );
}

/** Takes away everything this file drew. */
async function lower(): Promise<void> {
  await browser.execute(() => {
    for (const one of document.querySelectorAll('[data-raised="pane-notices.e2e"]')) one.remove();
  });
}

/** Every box this file needs of pane `pane` and the Notices raised in it. */
async function measured(pane: number) {
  return browser.execute((at: number) => {
    const box = (el: Element | null | undefined) => {
      if (!el) return null;
      const { left, right, top, bottom, width, height } = el.getBoundingClientRect();
      return { left, right, top, bottom, width, height };
    };
    const frames = [...document.querySelectorAll(".pane-frame")].filter((frame) =>
      frame.querySelector('[data-testid="pane"]'),
    );
    const frame = frames[at];
    const stack = frame?.querySelector<HTMLElement>(".pane-notices");
    const notices = [...(frame?.querySelectorAll('[data-raised="pane-notices.e2e"]') ?? [])].map(
      (raised) => {
        const line = raised.querySelector(".notice-pane");
        const pre = raised.querySelector<HTMLElement>("pre");
        return {
          box: box(raised),
          line: box(line),
          says: box(raised.querySelector(".notice-says")),
          buttons: [...(line?.querySelectorAll("button") ?? [])].map((button) => {
            const css = getComputedStyle(button);
            const lineHeight = Number.parseFloat(css.lineHeight);
            return {
              label: button.textContent ?? "",
              box: box(button),
              // One line of the button's text, as the explorer's rows are measured.
              line: Number.isFinite(lineHeight)
                ? lineHeight
                : Number.parseFloat(css.fontSize) * 1.5,
              chrome:
                Number.parseFloat(css.paddingTop) +
                Number.parseFloat(css.paddingBottom) +
                Number.parseFloat(css.borderTopWidth) +
                Number.parseFloat(css.borderBottomWidth),
            };
          }),
          under: box(raised.querySelector(".notice-under-pane")),
          pre: box(pre),
          preScrolls: pre ? pre.scrollHeight > pre.clientHeight + 1 : false,
          background: getComputedStyle(raised).backgroundColor,
        };
      },
    );
    return {
      window: { width: window.innerWidth, height: window.innerHeight },
      rem: Number.parseFloat(getComputedStyle(document.documentElement).fontSize),
      frame: box(frame),
      neighbours: frames.filter((one) => one !== frame).map((one) => box(one)),
      corner: box(frame?.querySelector(".pane-corner.at-start")),
      stack: box(stack),
      stackScrolls: stack ? stack.scrollHeight > stack.clientHeight + 1 : false,
      notices,
    };
  }, pane);
}

/**
 * One measured claim, failing with what was measured: this suite's `expect` takes no message,
 * and a bare "expected 412 to be at most 380" names neither the box nor the edge.
 */
function check(
  what: string,
  value: number | boolean,
  is: "atLeast" | "atMost" | "below" | "above" | "is",
  other: number | boolean,
) {
  const holds =
    is === "is"
      ? value === other
      : is === "atLeast"
        ? value >= other
        : is === "atMost"
          ? value <= other
          : is === "below"
            ? value < other
            : value > other;
  if (!holds) throw new Error(`${what}: ${String(value)} is not ${is} ${String(other)}`);
}

/** `inner` is inside `outer` on all four sides, to within a pixel of rounding. */
function inside(inner: Box | null, outer: Box | null, what: string) {
  check(`${what}: nothing was drawn`, inner !== null, "is", true);
  check(`${what}: nothing to be inside`, outer !== null, "is", true);
  const [a, b] = [inner as Box, outer as Box];
  check(`${what} has no width`, a.width, "above", 0);
  check(`${what} has no height`, a.height, "above", 0);
  check(`${what} starts left of it`, a.left, "atLeast", b.left - 1);
  check(`${what} runs past its right edge`, a.right, "atMost", b.right + 1);
  check(`${what} starts above it`, a.top, "atLeast", b.top - 1);
  check(`${what} runs past its bottom edge`, a.bottom, "atMost", b.bottom + 1);
}

/** Whether two boxes share any area. */
const overlap = (a: Box, b: Box) =>
  a.left < b.right - 1 && b.left < a.right - 1 && a.top < b.bottom - 1 && b.top < a.bottom - 1;

/**
 * How far anything is scrolled sideways, from the pane's frame up to the page. A focus scrolls
 * a `hidden` box to show what overflowed it, which is how the band and the sidebar came to be
 * cut at the left: every number here is 0 when nothing overflows.
 */
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

describe("a Notice in a pane's corner", () => {
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

  afterEach(async () => {
    await lower();
    await browser.execute(() => {
      for (const one of document.querySelectorAll('[data-raised="pane-notices.e2e:dialog"]'))
        one.remove();
    });
    const picker = await $('[role="dialog"][aria-labelledby="start-chat"]');
    if (await picker.isDisplayed().catch(() => false)) await browser.keys(["Escape"]);
  });

  // One app process serves the whole run: the window goes back to the size it had, and every
  // chat this file opened is ended (as `pane-fill.e2e.ts` ends its own).
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
      { timeout: 20_000, timeoutMsg: "the pane-notices spec left a chat open behind it" },
    );
  });

  /**
   * Asks for a window of this size and waits until the page has stopped changing width. What
   * the window became is the runner's to decide (a small screen gives less than it was asked
   * for), so each case below asserts the pane's width it needs and never the window's.
   */
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

  for (const pane of [0, 1]) {
    it(`fits pane ${pane} of a split in the narrowest window, sentence first and what it opened under it`, async () => {
      // 1024 px is the narrowest window purlis supports (ADR 0054), and with two panes side by
      // side each is a few hundred pixels: the operator's screenshot.
      await windowIs(1024, 768);
      expect(await raise(pane, { sentence: SENTENCE, ways: WAYS, opened: BRIEF })).toBe(true);

      const seen = await measured(pane);
      const [notice] = seen.notices;
      const frame = seen.frame as Box;
      expect(seen.notices).toHaveLength(1);
      expect(seen.neighbours).toHaveLength(1);
      // The case this is about: a pane narrower than a Notice would like to be.
      check("the pane is not a narrow one", frame.width, "below", 40 * seen.rem);

      // Inside its pane on all four sides, and so off its neighbour.
      inside(notice.box, frame, "the Notice in its pane");
      for (const neighbour of seen.neighbours)
        check(
          "it is over the pane beside it",
          overlap(notice.box as Box, neighbour as Box),
          "is",
          false,
        );
      expect((notice.box as Box).right).toBeLessThanOrEqual(seen.window.width);

      // The sentence has the row: as wide as the Notice, less its padding. It was one word
      // wide, and taller than it was wide.
      const says = notice.says as Box;
      const room = Math.min(frame.width - 16, 40 * seen.rem);
      expect(says.width).toBeGreaterThanOrEqual(room - 2 * seen.rem);
      expect(says.width).toBeGreaterThan(says.height);

      // Every way out is inside the Notice, under the sentence, and on one line or, where it
      // alone is nearly as wide as the pane, two. They were five lines each.
      expect(notice.buttons.map((one) => one.label)).toEqual(WAYS);
      for (const button of notice.buttons) {
        inside(button.box, notice.box, `"${button.label}" in the Notice`);
        check(
          `"${button.label}" is beside the sentence`,
          (button.box as Box).top,
          "atLeast",
          says.bottom - 1,
        );
        check(
          `"${button.label}" is broken over more than two lines`,
          (button.box as Box).height,
          "atMost",
          button.line * 2 + button.chrome + 4,
        );
        // A real target: about a line of text tall, or more.
        expect((button.box as Box).height).toBeGreaterThanOrEqual(button.line - 2);
      }
      // No two of them on top of each other.
      for (const [index, one] of notice.buttons.entries())
        for (const other of notice.buttons.slice(index + 1))
          check(
            `${one.label} / ${other.label}`,
            overlap(one.box as Box, other.box as Box),
            "is",
            false,
          );

      // What a way out opened is under the line, at the Notice's width: never beside it. Its
      // draft scrolls in its own box and widens nothing, however long its lines are.
      const [under, line, box] = [notice.under as Box, notice.line as Box, notice.box as Box];
      expect(under.top).toBeGreaterThanOrEqual(line.bottom - 1);
      expect(under.left).toBeGreaterThanOrEqual(box.left - 1);
      expect(under.right).toBeLessThanOrEqual(box.right + 1);
      inside(notice.pre, under, "the brief's box under the line");
      expect(notice.preScrolls).toBe(true);

      // Opaque: the terminal does not show through it.
      expect(notice.background).not.toMatch(/rgba\(.*,\s*0\)|transparent/);
    });
  }

  it("fits the dispatch question's five answers in a narrow pane, each whole and none over another", async () => {
    await windowIs(1024, 768);
    expect(await raise(0, { sentence: ASKS, ways: FIVE_WAYS, opened: BRIEF })).toBe(true);

    const seen = await measured(0);
    const [notice] = seen.notices;
    const frame = seen.frame as Box;
    expect(seen.notices).toHaveLength(1);
    check("the pane is not a narrow one", frame.width, "below", 40 * seen.rem);
    inside(notice.box, frame, "the Notice in its pane");

    // All five, in the order they are answered, under the sentence and inside the Notice.
    const says = notice.says as Box;
    expect(notice.buttons.map((one) => one.label)).toEqual(FIVE_WAYS);
    for (const button of notice.buttons) {
      inside(button.box, notice.box, `"${button.label}" in the Notice`);
      check(
        `"${button.label}" is beside the sentence`,
        (button.box as Box).top,
        "atLeast",
        says.bottom - 1,
      );
      check(
        `"${button.label}" is broken over more than two lines`,
        (button.box as Box).height,
        "atMost",
        button.line * 2 + button.chrome + 4,
      );
      expect((button.box as Box).height).toBeGreaterThanOrEqual(button.line - 2);
    }
    for (const [index, one] of notice.buttons.entries())
      for (const other of notice.buttons.slice(index + 1))
        check(
          `${one.label} / ${other.label}`,
          overlap(one.box as Box, other.box as Box),
          "is",
          false,
        );

    // The brief is still under all five, never pushed beside them or out of the pane.
    const [under, line] = [notice.under as Box, notice.line as Box];
    expect(under.top).toBeGreaterThanOrEqual(line.bottom - 1);
    inside(notice.pre, under, "the brief's box under the line");
  });

  it("stacks several in one pane, and scrolls them inside the pane when they are taller than it", async () => {
    await windowIs(1024, 768);
    // Six of the tallest: far more than any pane of this window holds.
    for (let one = 0; one < 6; one++)
      expect(await raise(0, { sentence: SENTENCE, ways: WAYS, opened: BRIEF })).toBe(true);

    const seen = await measured(0);
    const frame = seen.frame as Box;
    expect(seen.notices).toHaveLength(6);

    // The corner and its stack end inside the pane; the pane is as tall as its neighbour, so
    // it did not grow to hold what purlis has to say.
    inside(seen.corner, frame, "the pane's corner");
    inside(seen.stack, frame, "the stack of Notices");
    expect(seen.stackScrolls).toBe(true);
    expect(Math.abs(frame.height - (seen.neighbours[0] as Box).height)).toBeLessThanOrEqual(1);

    // One under another, in the order they were raised, with a gap and never overlapping.
    for (const [index, one] of seen.notices.entries()) {
      const next = seen.notices[index + 1];
      if (next === undefined) continue;
      expect((next.box as Box).top).toBeGreaterThan((one.box as Box).bottom);
      expect((next.box as Box).left).toBeCloseTo((one.box as Box).left, 0);
    }

    // The last one's last button is reachable: scrolled to, it is inside the pane.
    const reached = await browser.execute(() => {
      const buttons = document.querySelectorAll<HTMLElement>(
        '[data-raised="pane-notices.e2e"] .notice-pane > button',
      );
      const last = buttons[buttons.length - 1];
      last?.focus();
      last?.scrollIntoView({ block: "nearest" });
      const { left, right, top, bottom, width, height } = last.getBoundingClientRect();
      return { left, right, top, bottom, width, height };
    });
    inside(reached, frame, "the last Notice's last button, scrolled to");
  });

  it("scrolls nothing sideways when the keyboard goes through its buttons", async () => {
    // What cut the band and the sidebar at the left: a button that overflowed the pane was
    // focused, and every box up to the page was scrolled to show it.
    await windowIs(1024, 768);
    for (const pane of [0, 1])
      expect(await raise(pane, { sentence: SENTENCE, ways: WAYS, opened: BRIEF })).toBe(true);

    await browser.execute(() => {
      for (const button of document.querySelectorAll<HTMLElement>(
        '[data-raised="pane-notices.e2e"] button',
      )) {
        button.focus();
        button.scrollIntoView({ block: "nearest", inline: "nearest" });
      }
    });

    for (const pane of [0, 1]) {
      const scrolled = await scrolledSideways(pane);
      expect(scrolled.length).toBeGreaterThan(3);
      expect(scrolled.filter((by) => by !== 0)).toEqual([]);
    }
    // And the band and the sidebar start where the window does.
    const starts = await browser.execute(() =>
      [".title-bar", 'nav[aria-label="Explorer"]', ".status-line"].map(
        // A region that is put away is not cut: only one that is drawn is measured.
        (where) => document.querySelector(where)?.getBoundingClientRect().left ?? 0,
      ),
    );
    for (const left of starts) expect(left).toBeGreaterThanOrEqual(0);
  });

  it("is one row in a wide pane, as it was, and a long one is no wider than a sentence reads", async () => {
    await windowIs(1600, 1000);
    // The one-liner the corner was made for (ADR 0062), then the long one.
    const short = { sentence: "Restarting this chat…", ways: ["Dismiss"] };
    expect(await raise(0, short)).toBe(true);
    expect(await raise(0, { sentence: SENTENCE, ways: WAYS, opened: BRIEF })).toBe(true);

    const seen = await measured(0);
    const frame = seen.frame as Box;
    const [oneLine, long] = seen.notices;

    // One row: every button beside the sentence, to its right, and the box one line tall.
    check("the pane is not wide enough for one line", frame.width, "above", 24 * seen.rem);
    inside(oneLine.box, frame, "the one-line Notice in its pane");
    const says = oneLine.says as Box;
    for (const button of oneLine.buttons) {
      const at = button.box as Box;
      check(`"${button.label}" is not beside the sentence`, at.left, "atLeast", says.right - 1);
      expect(at.top).toBeLessThan(says.bottom);
      expect(at.height).toBeLessThanOrEqual(button.line * 1.5 + button.chrome + 4);
    }
    expect(says.height).toBeLessThanOrEqual(oneLine.buttons[0].line * 1.5);
    // As wide as what it says, not as wide as the pane.
    expect((oneLine.box as Box).width).toBeLessThan(frame.width - 32);

    // The long one: inside the pane, and capped where a sentence stops reading well.
    inside(long.box, frame, "the long Notice in its pane");
    expect((long.box as Box).width).toBeLessThanOrEqual(40 * seen.rem + 1);
    expect((long.under as Box).top).toBeGreaterThanOrEqual((long.line as Box).bottom - 1);
    expect((long.under as Box).right).toBeLessThanOrEqual((long.box as Box).right + 1);
    for (const neighbour of seen.neighbours)
      expect(overlap(long.box as Box, neighbour as Box)).toBe(false);
  });

  it("is over its terminal, and under a dialog and its scrim", async () => {
    await windowIs(1024, 768);
    expect(await raise(1, { sentence: SENTENCE, ways: WAYS, opened: BRIEF })).toBe(true);

    /** What is drawn at the middle of the Notice's sentence: the Notice, or something over it. */
    const atTheNotice = () =>
      browser.execute(() => {
        const says = document.querySelector('[data-raised="pane-notices.e2e"] .notice-says');
        if (!says) return "(no Notice)";
        const box = says.getBoundingClientRect();
        const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
        if (hit?.closest('[data-raised="pane-notices.e2e"]')) return "the Notice";
        if (hit?.closest('[role="dialog"]')) return "the dialog";
        if (hit?.closest(".asking")) return "the scrim";
        if (hit?.closest('[data-testid="pane"]')) return "the terminal";
        return hit?.className || hit?.tagName || "(nothing)";
      });

    // Over all of the terminal, whatever layers the terminal has of its own.
    expect(await atTheNotice()).toBe("the Notice");

    // A real dialog, portaled to the body as every dialog is: the picker. It was drawn UNDER
    // the pane's corner, so a Notice sat on the dialog's top edge.
    await pressOnly("New tab");
    const picker = await $('[role="dialog"][aria-labelledby="start-chat"]');
    await picker.waitForDisplayed({ timeout: 20_000 });
    expect(["the scrim", "the dialog"]).toContain(await atTheNotice());
    // And the dialog's own top edge is the dialog.
    const top = await browser.execute(() => {
      const dialog = document.querySelector('[role="dialog"][aria-labelledby="start-chat"]');
      if (!dialog) return false;
      const box = dialog.getBoundingClientRect();
      const hit = document.elementFromPoint(box.left + box.width / 2, box.top + 2);
      return hit !== null && dialog.contains(hit);
    });
    expect(top).toBe(true);
    await browser.keys(["Escape"]);
    await expect(picker).not.toBeDisplayed();
    expect(await atTheNotice()).toBe("the Notice");
  });

  it("keeps the dispatch question's workspace choice under its answers and inside a narrow pane", async () => {
    // #1505. Where an Allow holds is chosen in the box under the line, above the brief. Drawn
    // here with the classes `DispatchGrantNotice` draws, and a workspace named far longer than
    // the pane is wide.
    await windowIs(1024, 768);
    expect(await raise(0, { sentence: ASKS, ways: FIVE_WAYS, opened: BRIEF })).toBe(true);
    const drawn = await browser.execute((workspace: string) => {
      const under = document.querySelector('[data-raised="pane-notices.e2e"] .notice-under-pane');
      if (!under) return false;
      const choice = document.createElement("div");
      choice.className = "dispatch-within";
      choice.setAttribute("role", "radiogroup");
      const says = document.createElement("p");
      says.textContent = "Where an Allow for you or for the project holds";
      choice.append(says);
      for (const [index, text] of [`In ${workspace} only`, "In any workspace"].entries()) {
        const label = document.createElement("label");
        const radio = document.createElement("input");
        radio.type = "radio";
        radio.name = "pane-notices-within";
        radio.checked = index === 0;
        label.append(radio, ` ${text}`);
        choice.append(label);
      }
      under.prepend(choice);
      return true;
    }, "a_workspace_named_wider_than_a_pane_".repeat(6));
    expect(drawn).toBe(true);

    const seen = await measured(0);
    const [notice] = seen.notices;
    const frame = seen.frame as Box;
    check("the pane is not a narrow one", frame.width, "below", 40 * seen.rem);
    const parts = await browser.execute(() => {
      const box = (el: Element | null | undefined) => {
        if (!el) return null;
        const { left, right, top, bottom, width, height } = el.getBoundingClientRect();
        return { left, right, top, bottom, width, height };
      };
      const choice = document.querySelector('[data-raised="pane-notices.e2e"] .dispatch-within');
      return {
        choice: box(choice),
        says: box(choice?.querySelector("p")),
        labels: [...(choice?.querySelectorAll("label") ?? [])].map((label) => box(label)),
      };
    });

    // The Notice still fits its pane, and the choice is inside the box under the line.
    inside(notice.box, frame, "the Notice in its pane");
    inside(parts.choice, notice.under, "the workspace choice under the line");
    inside(parts.says, parts.choice, "what the choice is about");
    expect(parts.labels).toHaveLength(2);
    for (const [index, label] of parts.labels.entries())
      inside(label, parts.choice, `choice ${index + 1}`);
    // The long name broke: the two choices are one under the other, never one over the other.
    const [narrow, wide] = parts.labels as Box[];
    check("the two choices overlap", overlap(narrow, wide), "is", false);
    // Under every answer, and above the brief.
    const [line, choice] = [notice.line as Box, parts.choice as Box];
    check("the choice is beside the answers", choice.top, "atLeast", line.bottom - 1);
    check("the brief is above the choice", (notice.pre as Box).top, "atLeast", choice.bottom - 1);
  });
});

/**
 * **Ask {persona}, measured** (#1481): the dialog the operator met from "Dispatch to devops…".
 * Its "What to ask" box was a third of the dialog's width under a full-width name, and its
 * buttons were below the bottom of the window.
 *
 * Drawn as `AskPersona` draws it (see the note at the top of this file), where a dialog is
 * portaled: the end of the body.
 */
describe("the Ask persona dialog", () => {
  let was = { width: 1280, height: 800 };

  before(async () => {
    was = await browser.getWindowSize();
  });

  afterEach(async () => {
    await browser.execute(() => {
      for (const one of document.querySelectorAll('[data-raised="pane-notices.e2e:dialog"]'))
        one.remove();
    });
  });

  after(async () => {
    await browser.setWindowSize(was.width, was.height);
  });

  it("gives the box you write in the name's width, fits the window, and keeps its buttons in reach", async () => {
    await browser.setWindowSize(1024, 768);
    await browser.pause(500);

    const seen = await browser.execute(() => {
      const el = (tag: string, cls: string, text?: string) => {
        const made = document.createElement(tag);
        made.className = cls;
        if (text !== undefined) made.textContent = text;
        made.dataset.raised = "pane-notices.e2e:dialog";
        return made;
      };
      const row = (control: HTMLElement, help: string) => {
        const made = el("div", "ui-setting-row");
        const holder = el("div", "ui-setting-control");
        holder.append(control);
        made.append(
          el("label", "ui-setting-label", "A row"),
          holder,
          el("p", "ui-setting-help", help),
        );
        return made;
      };
      const scrim = el("div", "asking");
      const dialog = el("div", "warning ask-persona");
      dialog.setAttribute("role", "dialog");
      const name = el("input", "ui-field") as HTMLInputElement;
      const ask = el("textarea", "ui-field") as HTMLTextAreaElement;
      ask.rows = 4;
      const form = el("form", "");
      form.append(
        row(name, "What the new chat is called, on its tab and under this chat."),
        row(ask, "A chat starts as devops on these words, with its own sandbox, hosts and vaults."),
      );
      // Where it works, and then some: more than a window of this height shows at once.
      for (let more = 0; more < 24; more++)
        form.append(
          row(
            el("span", "", "A branch of its own"),
            "purlis cuts a new branch, in a folder of its own.",
          ),
        );
      const answers = el("div", "ui-setting-actions");
      answers.append(el("button", "", "Ask devops"), el("button", "", "Cancel"));
      form.append(answers);
      dialog.append(el("h2", "", "Ask devops"), form);
      document.body.append(scrim, dialog);

      const box = (of: Element) => {
        const { left, right, top, bottom, width, height } = of.getBoundingClientRect();
        return { left, right, top, bottom, width, height };
      };
      const style = getComputedStyle(ask);
      return {
        window: {
          left: 0,
          top: 0,
          right: window.innerWidth,
          bottom: window.innerHeight,
          width: window.innerWidth,
          height: window.innerHeight,
        },
        dialog: box(dialog),
        name: box(name),
        ask: box(ask),
        resize: style.resize,
        answers: box(answers),
        scrolls: dialog.scrollHeight > dialog.clientHeight + 1,
        scrolledTo: dialog.scrollTop,
      };
    });

    // The box you write in is as wide as the name's, and grows downwards only.
    expect(Math.abs(seen.ask.width - seen.name.width)).toBeLessThanOrEqual(1);
    expect(Math.abs(seen.ask.left - seen.name.left)).toBeLessThanOrEqual(1);
    expect(seen.ask.width).toBeGreaterThan(seen.dialog.width * 0.8);
    expect(seen.ask.height).toBeGreaterThan(seen.name.height * 2);
    expect(seen.resize).toBe("vertical");

    // The dialog is inside the window and scrolls inside itself; before any scrolling, its
    // answers are at its bottom edge, in view.
    inside(seen.dialog, seen.window, "the dialog in the window");
    expect(seen.scrolls).toBe(true);
    expect(seen.scrolledTo).toBe(0);
    inside(seen.answers, seen.dialog, "the dialog's answers, before it is scrolled");
    inside(seen.answers, seen.window, "the dialog's answers in the window");
  });
});
