import { browser, $ } from "@wdio/globals";
import { READY } from "../harness.js";
import { endChat, pressAndStart } from "../opening.js";

/**
 * **A row of the Chats list in a narrow sidebar, measured** (#1499, V100-50).
 *
 * The operator's screenshot: rows cut off at the sidebar's edge, "cancel mid-turn smart-ide ●
 * no…", with the state the first thing lost. A row is now two lines and as wide as the list:
 * the name gives way, and the state's word is whole.
 *
 * That is layout, and jsdom lays nothing out, so it is measured here: in the real engine,
 * against the built stylesheet, in the real Chats section.
 *
 * **What is drawn is the shape, not the cause.** A task that ended without a report needs a
 * persona chat and a core holding its record, which this suite's fake harness has no way to
 * make. So rows are drawn into the section's own list (`.chats-list`, which `ChatsSection`
 * draws around its tree) with the elements and classes a row has. `ChatsList.window.test.tsx`
 * holds the component to that shape; this file holds the stylesheet to what it must look
 * like. A class renamed in one place and not the other fails one of the two.
 */

/** The longest word a state has (`shownState.ts`). */
const LONGEST = "ended without a report";

type Drawn = { level: number; name: string; state: string; second: string };

const ROWS: Drawn[] = [
  { level: 1, name: "cancel mid-turn smart-ide", state: LONGEST, second: "smart-ide · 12m" },
  { level: 1, name: "steward 4", state: "needs you", second: "charter-app" },
  {
    level: 2,
    name: "live check of the staging cluster after the release",
    state: LONGEST,
    second: "volaticloud · 3h · own branch purlis/dispatch/live-check-of-the-staging-cluster",
  },
  { level: 2, name: "devops 12", state: "working", second: "" },
  {
    level: 3,
    name: "a_name_with_no_space_in_it_that_is_far_wider_than_a_sidebar",
    state: LONGEST,
    second: "",
  },
];

type Box = { left: number; right: number; top: number; bottom: number; width: number };

type Measured = {
  section: Box;
  /** How far the section scrolls sideways past what it shows: 0 when nothing overflows it. */
  overflow: number;
  rows: {
    name: string;
    item: Box;
    row: Box;
    session: Box;
    /** Whether the name is cut short: wider than the box it is drawn in. */
    nameCut: boolean;
    word: Box;
    /** How much of the word its own box does not show. */
    wordHidden: number;
    wordText: string;
  }[];
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

/** Draws `rows` into the Chats section's list, as `ChatsSection` draws a row, with the
 *  section held to `width` pixels. Answers whether there was a list to draw them into. */
async function draw(rows: Drawn[], width: number): Promise<boolean> {
  return browser.execute(
    (drawn: Drawn[], px: number) => {
      const section = document.querySelector<HTMLElement>(".chats-section");
      const list = section?.querySelector(".chats-list");
      if (!section || !list) return false;
      for (const one of document.querySelectorAll('[data-raised="chats-list.e2e"]')) one.remove();
      section.style.width = `${px}px`;
      section.style.maxHeight = "none";
      section.dataset.narrowed = "chats-list.e2e";
      const el = (tag: string, cls: string, text?: string) => {
        const made = document.createElement(tag);
        made.className = cls;
        if (text !== undefined) made.textContent = text;
        return made;
      };
      const tree = el("ul", "");
      tree.setAttribute("role", "tree");
      tree.dataset.raised = "chats-list.e2e";
      for (const one of drawn) {
        const item = el("li", "");
        item.setAttribute("role", "none");
        item.dataset.level = String(one.level);
        item.append(el("span", "twist"));
        const row = el("button", "chat");
        row.setAttribute("type", "button");
        row.setAttribute("role", "treeitem");
        const first = el("span", "line one");
        const mark = el("span", "persona-mark", "S");
        mark.style.width = "1rem";
        first.append(mark, el("span", "session", one.name));
        const state = el("span", "shown-state");
        const shape = el("span", "shape", "●");
        state.append(shape, el("span", "word", one.state));
        first.append(state);
        row.append(first);
        if (one.second !== "") {
          const second = el("span", "line two");
          second.append(el("span", "workspace", one.second));
          row.append(second);
        }
        item.append(row);
        tree.append(item);
      }
      list.append(tree);
      return true;
    },
    rows,
    width,
  );
}

async function lower(): Promise<void> {
  await browser.execute(() => {
    for (const one of document.querySelectorAll('[data-raised="chats-list.e2e"]')) one.remove();
    const section = document.querySelector<HTMLElement>('[data-narrowed="chats-list.e2e"]');
    if (section) {
      section.style.width = "";
      section.style.maxHeight = "";
      delete section.dataset.narrowed;
    }
  });
}

async function measured(): Promise<Measured | null> {
  return browser.execute(() => {
    const box = (el: Element) => {
      const { left, right, top, bottom, width } = el.getBoundingClientRect();
      return { left, right, top, bottom, width };
    };
    const section = document.querySelector<HTMLElement>(".chats-section");
    const tree = document.querySelector('[data-raised="chats-list.e2e"]');
    if (!section || !tree) return null;
    return {
      section: box(section),
      overflow: section.scrollWidth - section.clientWidth,
      rows: [...tree.querySelectorAll("li")].map((item) => {
        const row = item.querySelector(".chat") as HTMLElement;
        const session = item.querySelector(".session") as HTMLElement;
        const word = item.querySelector(".shown-state .word") as HTMLElement;
        return {
          name: session.textContent ?? "",
          item: box(item),
          row: box(row),
          session: box(session),
          nameCut: session.scrollWidth > session.clientWidth,
          word: box(word),
          wordHidden: word.scrollWidth - word.clientWidth,
          wordText: word.textContent ?? "",
        };
      }),
    };
  });
}

function check(what: string, holds: boolean, saw: unknown) {
  if (!holds) throw new Error(`${what}: ${JSON.stringify(saw)}`);
}

describe("a row of the Chats list in a narrow sidebar", () => {
  let wereAlreadyOpen: string[] = [];

  before(async () => {
    wereAlreadyOpen = await tabNames();
    // A chat, so the section draws its list.
    await pressAndStart("New tab");
    await browser.waitUntil(
      async () => (await $('[data-testid="pane"] .xterm-rows').getText()).includes(READY),
      { timeout: 30_000, interval: 250, timeoutMsg: "the chat never started" },
    );
    await $(".chats-section .chats-list").waitForExist({ timeout: 10_000 });
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

  for (const width of [150, 190, 240]) {
    it(`keeps the state's word whole and every row inside the list, at ${width}px`, async () => {
      check("there was no list to draw rows into", await draw(ROWS, width), width);
      const seen = await measured();
      check("nothing was measured", seen !== null, seen);
      const { section, overflow, rows } = seen as Measured;

      check("the section was not held to the width asked for", section.width <= width + 1, section);
      // No row makes the list scroll sideways.
      check("the list scrolls sideways", overflow <= 1, overflow);
      for (const one of rows) {
        // The word is whole: nothing of it is cut by its own box, the row, or the list's edge.
        check(`${one.name}: its state's word is cut short`, one.wordHidden <= 0, one);
        check(`${one.name}: its state is not the word drawn`, one.wordText !== "", one);
        check(`${one.name}: its state runs past the row`, one.word.right <= one.row.right + 1, one);
        check(
          `${one.name}: its state starts before the row`,
          one.word.left >= one.row.left - 1,
          one,
        );
        check(
          `${one.name}: its state runs past the list`,
          one.word.right <= section.right + 1 && one.word.left >= section.left - 1,
          one,
        );
        // The row is inside the list, and the name inside the row.
        check(`${one.name}: its row runs past the list`, one.item.right <= section.right + 1, one);
        check(
          `${one.name}: its name runs past the row`,
          one.session.right <= one.row.right + 1,
          one,
        );
      }
      // What gave way is the name: the long ones are cut short, with an ellipsis.
      for (const one of rows.filter((row) => row.name.length > 40))
        check(`${one.name}: its name was not what gave way`, one.nameCut, one);
    });
  }

  it("draws the state beside the name where there is room for both", async () => {
    check("there was no list to draw rows into", await draw(ROWS, 420), 420);
    const seen = (await measured()) as Measured;

    const short = seen.rows.find((row) => row.name === "steward 4");
    check("the short row was not drawn", short !== undefined, seen.rows);
    const { session, word } = short as Measured["rows"][number];
    // One line: the word starts right of the name and at its height.
    check("the state is not beside the name", word.left >= session.right - 1, short);
    check("the state is not on the name's line", word.top < session.bottom, short);
  });
});
