import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { browser, $ } from "@wdio/globals";
import { READY } from "../harness.js";
import { endChat, pressAndStart } from "../opening.js";

/**
 * **The Chats list in a narrow sidebar, measured** (#1499, V100-50).
 *
 * The operator's screenshot: rows cut off at the sidebar's edge, "cancel mid-turn smart-ide ●
 * no…", with the state the first thing lost. A row is now as wide as the list: the name gives
 * way, and the state's word is whole. That holds for a finished task's row too, at the least
 * width the left region can be dragged to, five levels down, at the largest text.
 *
 * That is layout, and jsdom lays nothing out, so it is measured here: in the real engine,
 * against the built stylesheet.
 *
 * **What is measured is the component's own markup.** This suite's fake harness cannot make
 * the chats these cases need (a task five levels down that ended without a report, a session
 * folded over five finished tasks). So `src/ChatsList.fixture.test.tsx` draws the real
 * `ChatsSection` with them and keeps what it drew in `e2e/fixtures/`, and fails when the
 * component stops drawing that. This file puts that markup into the window's left region, in
 * the real section's place, and measures it. Nothing here builds a row by hand.
 */

/** The least the left region can be dragged to (`SLOTS.left.floor` in `regions.ts`).
 *  `ChatsList.fixture.test.tsx` fails when this line and that one differ. */
const FLOOR = "11rem";
/** The largest text the window can be set to (`textSize.ts`), held the same way. */
const MOST_TEXT = 24;

const fixture = (name: string) =>
  readFileSync(fileURLToPath(new URL(`../fixtures/${name}`, import.meta.url)), "utf8");
const TWO_LINES = fixture("chats-list.two-lines.html");
const ONE_LINE = fixture("chats-list.one-line.html");

type Box = { left: number; right: number; top: number; bottom: number; width: number };

type Word = {
  /** The row it is on, by its name. */
  row: string;
  text: string;
  box: Box;
  /** How much of the word its own box does not show: 0 when it is whole. */
  cut: number;
  /** Whether any box above it, up to the section, clips it and is narrower than it. */
  clipped: boolean;
};

type Measured = {
  section: Box;
  /** How far the section scrolls sideways past what it shows: 0 when nothing overflows it. */
  overflow: number;
  /** Every state word drawn: on a chat's row and on a finished task's. */
  words: Word[];
  /** Every row's own box: a chat's item, a finished task's line. */
  rows: { name: string; box: Box; nameCut: boolean }[];
};

async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [
      ...(document
        .querySelector('[role="tablist"][aria-label="Tabs"]')
        ?.querySelectorAll('[role="tab"]') ?? []),
    ].map((tab) => tab.querySelector(".tab-name")?.textContent ?? ""),
  );
}

/**
 * Puts the component's markup in the real section's place, held to `width` (any CSS length),
 * with the window's text at `text` px where one is given. Answers whether there was a left
 * region to put it in.
 */
async function draw(html: string, width: string, text?: number): Promise<boolean> {
  return browser.execute(
    (markup: string, wide: string, px: number | null) => {
      const real = document.querySelector<HTMLElement>(
        '.left-region > [data-testid="chats-section"]',
      );
      if (!real) return false;
      for (const one of document.querySelectorAll('[data-raised="chats-list.e2e"]')) one.remove();
      if (px !== null) document.documentElement.style.fontSize = `${px}px`;
      const holder = document.createElement("div");
      holder.innerHTML = markup;
      const drawn = holder.firstElementChild as HTMLElement | null;
      if (!drawn) return false;
      drawn.dataset.raised = "chats-list.e2e";
      drawn.removeAttribute("data-testid");
      drawn.style.width = wide;
      // All of it, not two fifths of the region: every row is measured.
      drawn.style.maxHeight = "none";
      drawn.style.flex = "none";
      real.style.display = "none";
      real.before(drawn);
      return true;
    },
    html,
    width,
    text ?? null,
  );
}

async function lower(): Promise<void> {
  await browser.execute(() => {
    for (const one of document.querySelectorAll('[data-raised="chats-list.e2e"]')) one.remove();
    document.documentElement.style.fontSize = "";
    const real = document.querySelector<HTMLElement>(
      '.left-region > [data-testid="chats-section"]',
    );
    if (real) real.style.display = "";
  });
}

async function measured(): Promise<Measured | null> {
  return browser.execute(() => {
    const box = (el: Element) => {
      const { left, right, top, bottom, width } = el.getBoundingClientRect();
      return { left, right, top, bottom, width };
    };
    const section = document.querySelector<HTMLElement>('[data-raised="chats-list.e2e"]');
    if (!section) return null;
    const nameOf = (el: Element) =>
      el.closest("li, .finished-task")?.querySelector(".session")?.textContent ?? "";
    return {
      section: box(section),
      overflow: section.scrollWidth - section.clientWidth,
      words: [...section.querySelectorAll<HTMLElement>(".shown-state .word")].map((word) => {
        const mine = word.getBoundingClientRect();
        let clipped = false;
        for (let up = word.parentElement; up && up !== section; up = up.parentElement) {
          const style = getComputedStyle(up);
          if (style.overflowX === "visible") continue;
          const theirs = up.getBoundingClientRect();
          if (mine.left < theirs.left - 1 || mine.right > theirs.right + 1) clipped = true;
        }
        return {
          row: nameOf(word),
          text: word.textContent ?? "",
          box: box(word),
          cut: word.scrollWidth - word.clientWidth,
          clipped,
        };
      }),
      rows: [
        ...section.querySelectorAll<HTMLElement>(
          'li[role="none"]:not(.finished-tasks), .finished-line',
        ),
      ].map((row) => {
        const session = row.querySelector<HTMLElement>(".session");
        return {
          name: session?.textContent ?? "",
          box: box(row),
          nameCut: session !== null && session.scrollWidth > session.clientWidth,
        };
      }),
    };
  });
}

/** The top of every row and finished line, top to bottom. */
async function tops(): Promise<number[]> {
  return browser.execute(() =>
    [
      ...document.querySelectorAll(
        '[data-raised="chats-list.e2e"] li[role="none"], [data-raised="chats-list.e2e"] .finished-line',
      ),
    ].map((row) => Math.round(row.getBoundingClientRect().top)),
  );
}

function check(what: string, holds: boolean, saw: unknown) {
  if (!holds) throw new Error(`${what}: ${JSON.stringify(saw)}`);
}

/** Every state word is whole and inside the list, no row runs past it, and it does not scroll
 *  sideways. */
function whole(seen: Measured | null, least: number) {
  check("nothing was measured", seen !== null, seen);
  const { section, overflow, words, rows } = seen as Measured;
  check("no state word was drawn", words.length >= least, words.length);
  check("the list scrolls sideways", overflow <= 1, overflow);
  for (const one of words) {
    check(`${one.row}: its state is not a word`, one.text !== "", one);
    check(`${one.row}: its state's word has no width`, one.box.width > 0, one);
    check(`${one.row}: its state's word is cut short by its own box`, one.cut <= 1, one);
    check(`${one.row}: its state's word is cut short by a box around it`, !one.clipped, one);
    check(
      `${one.row}: its state's word runs past the list`,
      one.box.left >= section.left - 1 && one.box.right <= section.right + 1,
      { one, section },
    );
  }
  for (const one of rows)
    check(`${one.name}: its row runs past the list`, one.box.right <= section.right + 1, {
      one,
      section,
    });
  return seen as Measured;
}

describe("the Chats list in a narrow sidebar", () => {
  let wereAlreadyOpen: string[] = [];

  before(async () => {
    wereAlreadyOpen = await tabNames();
    // A chat, so the left region draws its Chats section with a list in it.
    await pressAndStart("New tab");
    await browser.waitUntil(
      async () => (await $('[data-testid="pane"] .xterm-rows').getText()).includes(READY),
      { timeout: 30_000, interval: 250, timeoutMsg: "the chat never started" },
    );
    await $('.left-region > [data-testid="chats-section"]').waitForExist({ timeout: 10_000 });
  });

  afterEach(lower);

  after(async () => {
    for (let round = 0; round < 5; round++) {
      const mine = (await tabNames()).filter((tab) => !wereAlreadyOpen.includes(tab));
      if (mine.length === 0) break;
      for (const name of mine) await endChat(`End chat ${name}`);
      await browser.pause(500);
    }
  });

  it("is never narrower than its floor: the left region's own least width", async () => {
    // What the panel is told, read back off it: the slot cannot be dragged under this.
    const least = await browser.execute(() => {
      const slot = document.querySelector<HTMLElement>(".slot-left");
      const root = Number.parseFloat(getComputedStyle(document.documentElement).fontSize);
      return { width: slot?.getBoundingClientRect().width ?? 0, root };
    });
    const floor = Number.parseFloat(FLOOR) * least.root;

    check("the left region is narrower than its floor", least.width >= floor - 1, {
      least,
      floor,
    });
  });

  for (const [what, html] of [
    ["two lines", TWO_LINES],
    ["one line", ONE_LINE],
  ] as const) {
    it(`keeps every state's word whole at the region's least width, on ${what}`, async () => {
      check("there was no left region to draw in", await draw(html, FLOOR), FLOOR);
      const seen = whole(await measured(), 10);

      // The longest state there is, five levels down and on a finished task's row.
      const longest = seen.words.filter((word) => word.text === "ended without a report");
      check("the longest state was not drawn three times", longest.length >= 3, longest);
      // What gave way is the name: the long ones are cut short, with an ellipsis.
      for (const one of seen.rows.filter((row) => row.name.length > 40))
        check(`${one.name}: its name was not what gave way`, one.nameCut, one);
    });

    it(`keeps every state's word whole at the least width and the largest text, on ${what}`, async () => {
      check("there was no left region to draw in", await draw(html, FLOOR, MOST_TEXT), FLOOR);

      whole(await measured(), 10);
    });
  }

  it("holds below its floor too, where a window was left narrower by an older layout", async () => {
    check("there was no left region to draw in", await draw(TWO_LINES, "150px"), 150);

    whole(await measured(), 10);
  });

  it("moves no row when a row or the line under the filter comes to say something", async () => {
    check("there was no left region to draw in", await draw(TWO_LINES, "16rem"), "16rem");
    const before = await tops();
    check("no row was drawn", before.length >= 8, before);

    // What a row says later: the time in its state on a second line that had nothing on it,
    // why a key did nothing, and what the filter hides.
    const filled = await browser.execute(() => {
      const section = document.querySelector('[data-raised="chats-list.e2e"]');
      const empty = [...(section?.querySelectorAll(".chat .line.two") ?? [])].filter(
        (line) => line.textContent === "",
      );
      for (const line of empty) {
        const since = document.createElement("span");
        since.className = "since";
        since.textContent = "just now";
        line.append(since);
      }
      const said = section?.querySelector(".chats-said");
      if (said)
        said.textContent =
          "purlis cannot open a chat beside another yet. Press Enter to open it in front.";
      return empty.length;
    });
    check("no row had an empty second line to fill", filled >= 1, filled);

    const after = await tops();
    check("a row moved", JSON.stringify(after) === JSON.stringify(before), { before, after });
  });

  it("moves no row when a working chat's line comes, changes and goes (#1493)", async () => {
    check("there was no left region to draw in", await draw(TWO_LINES, "16rem"), "16rem");
    const before = await tops();
    check("no row was drawn", before.length >= 8, before);

    // The fixture has rows that say what their chat is doing. Each second line is one line
    // high whatever it holds.
    const lines = await browser.execute(() => {
      const section = document.querySelector('[data-raised="chats-list.e2e"]');
      const two = [...(section?.querySelectorAll(".chat .line.two") ?? [])];
      return {
        doing: section?.querySelectorAll(".chat .line.two .chat-doing").length ?? 0,
        heights: two.map((line) => Math.round(line.getBoundingClientRect().height)),
      };
    });
    check("no row said what its chat is doing", lines.doing >= 1, lines);
    check(
      "a second line is not one line high",
      new Set(lines.heights).size === 1 && lines.heights[0] > 0,
      lines,
    );

    // A line far longer than the sidebar, on every row: cut short, on one line.
    await browser.execute(() => {
      const section = document.querySelector('[data-raised="chats-list.e2e"]');
      for (const line of section?.querySelectorAll(".chat .line.two") ?? []) {
        const doing = document.createElement("span");
        doing.className = "chat-doing";
        doing.textContent = `editing ${"a_file_name_that_is_long".repeat(2)}.tsx`;
        line.replaceChildren(doing);
      }
    });
    const during = await tops();
    check("a row moved when its line came", JSON.stringify(during) === JSON.stringify(before), {
      before,
      during,
    });
    const seen = await measured();
    check("a line made the list scroll sideways", seen !== null && seen.overflow <= 1, seen);

    // The turn ends: the line goes, and what was there comes back.
    await browser.execute(() => {
      const section = document.querySelector('[data-raised="chats-list.e2e"]');
      for (const line of section?.querySelectorAll(".chat .line.two") ?? []) {
        const workspace = document.createElement("span");
        workspace.className = "workspace";
        workspace.textContent = "smart-ide";
        line.replaceChildren(workspace);
      }
    });
    const after = await tops();
    check("a row moved when its line went", JSON.stringify(after) === JSON.stringify(before), {
      before,
      after,
    });
  });

  it("draws the state beside the name where there is room for both", async () => {
    check("there was no left region to draw in", await draw(TWO_LINES, "420px"), 420);
    const seen = whole(await measured(), 10);

    const row = await browser.execute(() => {
      const section = document.querySelector('[data-raised="chats-list.e2e"]');
      const chat = [...(section?.querySelectorAll(".chat") ?? [])].find(
        (one) => one.querySelector(".session")?.textContent === "steward 7",
      );
      const session = chat?.querySelector(".session")?.getBoundingClientRect();
      const word = chat?.querySelector(".shown-state .word")?.getBoundingClientRect();
      if (!session || !word) return null;
      return {
        nameRight: session.right,
        nameBottom: session.bottom,
        wordLeft: word.left,
        wordTop: word.top,
      };
    });
    check("the short row was not drawn", row !== null, seen.rows);
    // One line: the word starts right of the name and at its height.
    check(
      "the state is not beside the name",
      (row?.wordLeft ?? 0) >= (row?.nameRight ?? 0) - 1,
      row,
    );
    check("the state is not on the name's line", (row?.wordTop ?? 0) < (row?.nameBottom ?? 0), row);
  });
});
