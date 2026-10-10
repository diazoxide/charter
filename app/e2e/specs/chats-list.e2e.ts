import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { browser, $ } from "@wdio/globals";
import { READY } from "../harness.js";
import { endChat, pressAndStart } from "../opening.js";

/**
 * **The Chats list in a narrow sidebar, measured** (#1499, V100-50, #1675).
 *
 * The operator's screenshot: rows cut off at the sidebar's edge, "cancel mid-turn smart-ide ●
 * no…", with the state the first thing lost. A row is now one line as wide as the list: its
 * state's mark first, then the persona's badge and the name, and the name gives way. The mark
 * is whole, a finished task's as a chat's (#1687), at the least width the left region can be
 * dragged to, five levels down, at the largest text.
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
  /** Every finished task's state mark, measured as a word is: a finished row is one line too,
   *  its word heard and in its tooltip (#1687). */
  words: Word[];
  /** Every chat row's state mark, measured as a word is. */
  marks: Word[];
  /** Every row's own box: a chat's item, a finished task's line. */
  rows: { name: string; box: Box; nameCut: boolean }[];
};

async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [
      ...(document.querySelector('[data-strip="Tabs"]')?.querySelectorAll('[role="tab"]') ?? []),
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
        '.region-view > [data-testid="chats-section"]',
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
      '.region-view > [data-testid="chats-section"]',
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
    /** How whole `el` is drawn, and inside what. */
    const seen = (el: HTMLElement, text: string) => {
      const mine = el.getBoundingClientRect();
      let clipped = false;
      for (let up = el.parentElement; up && up !== section; up = up.parentElement) {
        const style = getComputedStyle(up);
        if (style.overflowX === "visible") continue;
        const theirs = up.getBoundingClientRect();
        if (mine.left < theirs.left - 1 || mine.right > theirs.right + 1) clipped = true;
      }
      return {
        row: nameOf(el),
        text,
        box: box(el),
        cut: el.scrollWidth - el.clientWidth,
        clipped,
      };
    };
    return {
      section: box(section),
      overflow: section.scrollWidth - section.clientWidth,
      words: [...section.querySelectorAll<HTMLElement>(".finished-name .shown-state .shape")].map(
        (mark) => seen(mark, mark.closest(".shown-state")?.getAttribute("data-state") ?? ""),
      ),
      marks: [...section.querySelectorAll<HTMLElement>(".chat .shown-state .shape")].map((mark) =>
        seen(mark, mark.closest(".shown-state")?.getAttribute("data-state") ?? ""),
      ),
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

/** Every state mark and word is whole and inside the list, no row runs past it, and it does
 *  not scroll sideways. */
function whole(seen: Measured | null, least: number) {
  check("nothing was measured", seen !== null, seen);
  const { section, overflow, words, marks, rows } = seen as Measured;
  check("no state mark was drawn", marks.length >= least, marks.length);
  check("no finished task's mark was drawn", words.length >= 1, words.length);
  check("the list scrolls sideways", overflow <= 1, overflow);
  for (const one of [...words, ...marks]) {
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
    await $('.region-view > [data-testid="chats-section"]').waitForExist({ timeout: 10_000 });
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

  it("keeps every state's mark whole at the region's least width", async () => {
    check("there was no left region to draw in", await draw(ONE_LINE, FLOOR), FLOOR);
    const seen = whole(await measured(), 10);

    // A finished task that ended without a report, five levels down, keeps its mark whole.
    const unreported = seen.words.filter((word) => word.text === "unreported");
    check("the unreported end was not drawn", unreported.length >= 1, seen.words);
    // What gave way is the name: the long ones are cut short, with an ellipsis.
    for (const one of seen.rows.filter((row) => row.name.length > 40))
      check(`${one.name}: its name was not what gave way`, one.nameCut, one);
  });

  it("keeps every state's mark whole at the least width and the largest text", async () => {
    check("there was no left region to draw in", await draw(ONE_LINE, FLOOR, MOST_TEXT), FLOOR);

    whole(await measured(), 10);
  });

  it("holds below its floor too, where a window was left narrower by an older layout", async () => {
    check("there was no left region to draw in", await draw(ONE_LINE, "150px"), 150);

    whole(await measured(), 10);
  });

  it("moves no row when a row or the line under the filter comes to say something", async () => {
    check("there was no left region to draw in", await draw(ONE_LINE, "16rem"), "16rem");
    const before = await tops();
    check("no row was drawn", before.length >= 8, before);

    // What a row says changes while the row stands: a folded row's count comes or goes, and a
    // chat is being stopped. Here every other row gains a count and the rest lose theirs; and
    // the line under the filter says why a key did nothing.
    const changed = await browser.execute(() => {
      const section = document.querySelector('[data-raised="chats-list.e2e"]');
      const lines = [...(section?.querySelectorAll(".chat .line.one") ?? [])];
      lines.forEach((line, at) => {
        const had = line.querySelector(".task-count");
        if (at % 2 === 1) {
          had?.remove();
          return;
        }
        if (had) return;
        const count = document.createElement("span");
        count.className = "task-count";
        count.textContent = "12";
        const stopping = document.createElement("span");
        stopping.className = "stopping";
        stopping.textContent = "Stopping…";
        line.append(count, stopping);
      });
      const said = section?.querySelector(".chats-said");
      if (said)
        said.textContent =
          "purlis cannot open a chat beside another yet. Press Enter to open it in front.";
      return lines.length;
    });
    check("no row had a line to change", changed >= 2, changed);

    const after = await tops();
    check("a row moved", JSON.stringify(after) === JSON.stringify(before), { before, after });
  });

  it("draws every chat's row one line high, its mark first and its name cut short (#1675)", async () => {
    check("there was no left region to draw in", await draw(ONE_LINE, FLOOR), FLOOR);
    whole(await measured(), 10);

    const rows = await browser.execute(() => {
      const section = document.querySelector('[data-raised="chats-list.e2e"]');
      return [...(section?.querySelectorAll<HTMLElement>(".chat") ?? [])].map((chat) => {
        const line = chat.querySelector(".line.one")?.getBoundingClientRect();
        const mark = chat.querySelector(".shown-state .shape")?.getBoundingClientRect();
        const name = chat.querySelector(".session")?.getBoundingClientRect();
        return {
          name: chat.querySelector(".session")?.textContent ?? "",
          height: Math.round(chat.getBoundingClientRect().height),
          lineHeight: Math.round(line?.height ?? 0),
          markLeft: mark?.left ?? 0,
          nameLeft: name?.left ?? 0,
          markTop: mark?.top ?? 0,
          nameBottom: name?.bottom ?? 0,
        };
      });
    });
    check("no chat row was drawn", rows.length >= 8, rows);
    // One line each, and all the same height, however long the name.
    check(
      "a chat's row is not one line high",
      new Set(rows.map((one) => one.height)).size === 1 && rows[0].height > 0,
      rows,
    );
    for (const one of rows) {
      check(`${one.name}: its mark is not before its name`, one.markLeft < one.nameLeft, one);
      check(`${one.name}: its mark is not on its name's line`, one.markTop < one.nameBottom, one);
    }
  });
});
