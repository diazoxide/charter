import { browser, expect, $$ } from "@wdio/globals";
import { READY } from "../harness.js";
import { endChat, pressAndStart } from "../opening.js";

/**
 * **A chat's Activity tab, measured in the real WebView** (#1457, the follow-ups of #1495).
 *
 * Three things here are the engine's and not jsdom's: that a task's lines are set in under the
 * chat that asked, one step for each level; that a clipped line opens in full inside its column,
 * a long word broken and the page never wider; and that the "also names" mark is drawn, its
 * sign in the waiting state's colour and its sentence readable on the tab. What each line says,
 * and what each press does, is `Activity.window.test.tsx`'s.
 *
 * **The timeline is pretended**, as `task-own-tab.e2e.ts` pretends its task: a timeline is read
 * from dispatch records, which need a persona chat and a core that dispatched a task, and this
 * suite's fake harness can make neither. The `e2e` build hands the open tab the core's answer
 * (`src/e2eActivity.ts`), and from there every line is drawn by the tab itself.
 *
 * **Presses are dispatched as clicks**, for the reason `workspace-explorer.e2e.ts` gives of the
 * embedded driver's pointer.
 */

const STRIP = '[data-strip="Tabs"]';
const PLACE = "project root\u0000.";

/** A clipped line: more rows than a line shows, and one word longer than any column. */
const LONG = [
  "Read the probe's config.",
  "Checked the cache.",
  "Checked the queue.",
  "Checked the store.",
  "Checked the router.",
  "Checked the logs.",
  "Checked the alerts.",
  "Checked the backups.",
  `And this: ${"x".repeat(320)}`,
].join("\n");

type Line = Record<string, unknown>;

/** One line as the core hands it over, at `depth` under the session, its chats all closed. */
function line(over: Line): Line {
  return {
    dispatch: "01K6PROBE",
    n: 0,
    at: "2026-10-08T09:00:00+00:00",
    kind: "note",
    from: "probe",
    from_key: "01K6PROBEKEY",
    from_session: null,
    to: "its session",
    to_key: "01K6SESSIONKEY",
    text: "",
    by_person: false,
    asks: null,
    answers: null,
    unread: false,
    by_purlis: false,
    expired: false,
    left_out: false,
    unkept: null,
    unkept_why: null,
    outcome: null,
    files: [],
    task: "probe",
    place: PLACE,
    depth: 1,
    ...over,
  };
}

/** The session dispatched probe and lint; probe dispatched dig, whose note is long. Dig and lint ran together, and
 *  both reports name one file. */
const LINES: Line[] = [
  line({
    n: 0,
    kind: "dispatched",
    text: "Is the rollout healthy?",
    from: "its session",
    from_key: "01K6SESSIONKEY",
    to: "probe",
    to_key: "01K6PROBEKEY",
  }),
  line({
    dispatch: "01K6DIG",
    n: 0,
    kind: "dispatched",
    text: "Dig into the cache.",
    at: "2026-10-08T09:01:00+00:00",
    to: "dig",
    to_key: "01K6DIGKEY",
    task: "dig",
    depth: 2,
  }),
  line({
    dispatch: "01K6DIG",
    n: 1,
    kind: "note",
    text: LONG,
    at: "2026-10-08T09:02:00+00:00",
    from: "dig",
    from_key: "01K6DIGKEY",
    to: "probe",
    to_key: "01K6PROBEKEY",
    task: "dig",
    depth: 2,
  }),
  line({
    dispatch: "01K6LINT",
    n: 0,
    kind: "dispatched",
    text: "Lint it.",
    at: "2026-10-08T09:02:30+00:00",
    from: "its session",
    from_key: "01K6SESSIONKEY",
    to: "lint",
    to_key: "01K6LINTKEY",
    task: "lint",
  }),
  line({
    dispatch: "01K6DIG",
    n: 2,
    kind: "report",
    outcome: "done",
    text: "Fixed the probe.",
    at: "2026-10-08T09:03:00+00:00",
    from: "dig",
    from_key: "01K6DIGKEY",
    to: "probe",
    to_key: "01K6PROBEKEY",
    task: "dig",
    depth: 2,
    files: ["src/app.rs"],
  }),
  line({
    dispatch: "01K6LINT",
    n: 1,
    kind: "report",
    outcome: "done",
    text: "Formatted.",
    at: "2026-10-08T09:04:00+00:00",
    from: "lint",
    from_key: "01K6LINTKEY",
    task: "lint",
    files: ["src/app.rs"],
  }),
];

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

/** Opens the Activity of the chat in the tab called `name`, from that tab's own menu. */
async function openActivityOf(name: string): Promise<void> {
  const sent = await browser.execute(
    (strip: string, called: string) => {
      const tab = [...document.querySelectorAll<HTMLElement>(`${strip} [role="tab"]`)].find(
        (one) => one.querySelector(".tab-name")?.textContent === called,
      );
      if (!tab) return false;
      const box = tab.getBoundingClientRect();
      tab.dispatchEvent(
        new MouseEvent("contextmenu", {
          bubbles: true,
          cancelable: true,
          clientX: Math.round(box.left + box.width / 2),
          clientY: Math.round(box.top + box.height / 2),
        }),
      );
      return true;
    },
    STRIP,
    name,
  );
  expect(sent).toBe(true);
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        // By the row's name: its text also holds the note under the title (Menus.tsx).
        const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(
          (one) => one.getAttribute("aria-label") === "Activity",
        );
        item?.click();
        return item !== undefined;
      }),
    { timeout: 10_000, interval: 100, timeoutMsg: `${name}'s menu never offered Activity` },
  );
  // The core's own read has answered, whatever it said, before the timeline is handed over.
  await browser.waitUntil(
    async () =>
      browser.execute(
        () =>
          document.querySelector(
            '[data-testid="activity-empty"], [data-testid="activity-closed"], ol.activity',
          ) !== null,
      ),
    { timeout: 20_000, interval: 100, timeoutMsg: "the Activity tab never answered its read" },
  );
}

/** Hands the open Activity tab the timeline, as the core would answer it. */
async function pretend(lines: Line[]): Promise<void> {
  await browser.execute((said: Line[]) => {
    window.dispatchEvent(
      new CustomEvent("purlis-e2e-activity", {
        detail: {
          name: "its session",
          key: "01K6SESSIONKEY",
          lines: said,
          undrawn: 0,
          unlisted: 0,
          unread: 0,
          most_listed: 200,
          most_read: 2000,
        },
      }),
    );
  }, lines);
  await browser.waitUntil(
    async () => browser.execute(() => document.querySelectorAll("ol.activity > li").length === 6),
    { timeout: 10_000, interval: 100, timeoutMsg: "the pretended timeline was never drawn" },
  );
}

/** Each line, measured: its depth, where its time starts, its edges, and the list's. */
async function measured() {
  return browser.execute(() => {
    const list = document.querySelector<HTMLElement>("ol.activity");
    if (!list) throw new Error("no timeline is drawn");
    const listBox = list.getBoundingClientRect();
    return {
      list: { left: listBox.left, right: listBox.right },
      sideways: list.scrollWidth - list.clientWidth,
      page: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      rem: parseFloat(getComputedStyle(document.documentElement).fontSize),
      lines: [...list.querySelectorAll<HTMLElement>(":scope > li")].map((item) => {
        const at = item.querySelector(".activity-at")?.getBoundingClientRect();
        const text = item.querySelector(".activity-text")?.getBoundingClientRect();
        const box = item.getBoundingClientRect();
        return {
          depth: Number(item.dataset.depth),
          at: at?.left ?? NaN,
          right: box.right,
          height: box.height,
          textRight: text?.right ?? NaN,
        };
      }),
    };
  });
}

/** The clipped line: what it shows, its control, and its height. */
async function clippedLine() {
  return browser.execute(() => {
    const item = [...document.querySelectorAll<HTMLElement>("ol.activity > li")].find(
      (one) => one.querySelector(".activity-more") !== null,
    );
    if (!item) return null;
    const more = item.querySelector<HTMLElement>(".activity-more");
    return {
      text: item.querySelector(".activity-text")?.textContent ?? "",
      says: more?.textContent ?? "",
      expanded: more?.getAttribute("aria-expanded") ?? "",
      height: item.getBoundingClientRect().height,
    };
  });
}

/** A CSS colour as the engine says it (`rgb(…)` or `rgba(…)`): red, green, blue and alpha. */
function rgb(css: string): [number, number, number, number] {
  const parts = (css.match(/[\d.]+/g) ?? []).map(Number);
  return [parts[0] ?? 0, parts[1] ?? 0, parts[2] ?? 0, parts[3] ?? 1];
}

/** WCAG's relative luminance of a colour. */
function luminance([r, g, b]: number[]): number {
  const linear = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

/** WCAG's contrast ratio of two colours. */
function contrast(one: number[], other: number[]): number {
  const [hi, lo] = [luminance(one), luminance(other)].sort((a, b) => b - a);
  return (hi + 0.05) / (lo + 0.05);
}

/** Each "also names" mark: what it says, its sign's size, and the colours and their contrast. */
async function marks() {
  // Only colours are read in the page, and the contrast is worked out here: no named helpers
  // inside `execute`, because the spec's bundler wraps them in a `__name` the page does not have.
  const read = await browser.execute(() => {
    // What `--state-waiting` is drawn as, read the way the engine resolves it.
    const probe = document.createElement("span");
    probe.style.color = "var(--state-waiting)";
    document.body.appendChild(probe);
    const waiting = getComputedStyle(probe).color;
    probe.remove();
    return [...document.querySelectorAll<HTMLElement>(".activity-shared")].map((mark) => {
      const sign = mark.querySelector("svg");
      const box = sign?.getBoundingClientRect();
      // The colour drawn behind the mark: the nearest background that is not see-through.
      let ground = "rgb(255, 255, 255)";
      for (let at: Element | null = mark; at; at = at.parentElement) {
        const colour = getComputedStyle(at).backgroundColor;
        const alpha = colour.startsWith("rgba")
          ? Number((colour.match(/[\d.]+/g) ?? [])[3] ?? 1)
          : colour.startsWith("rgb")
            ? 1
            : 0;
        if (alpha > 0.99) {
          ground = colour;
          break;
        }
      }
      return {
        says: mark.textContent ?? "",
        sign: { width: box?.width ?? 0, height: box?.height ?? 0 },
        signColour: sign ? getComputedStyle(sign).color : "",
        waiting,
        words: getComputedStyle(mark).color,
        ground,
      };
    });
  });
  return read.map((one) => ({
    says: one.says,
    sign: one.sign,
    signIsWaiting: one.signColour === one.waiting,
    signContrast: contrast(rgb(one.signColour), rgb(one.ground)),
    wordsContrast: contrast(rgb(one.words), rgb(one.ground)),
  }));
}

describe("a chat's Activity tab", () => {
  let wereAlreadyOpen: string[] = [];
  let mine = "";

  before(async () => {
    wereAlreadyOpen = await tabNames();
    await pressAndStart("New tab");
    await untilShows(READY);
    [mine] = (await tabNames()).filter((tab) => !wereAlreadyOpen.includes(tab));
    await openActivityOf(mine);
    await pretend(LINES);
  });

  // One app process serves the whole run: the Activity tab is closed, and the chat this file
  // opened is ended.
  after(async () => {
    await browser.execute((strip: string) => {
      for (const closer of document.querySelectorAll<HTMLElement>(
        `${strip} button[aria-label^="Close Activity"]`,
      ))
        closer.click();
    }, STRIP);
    for (let round = 0; round < 5; round++) {
      const left = (await tabNames()).filter((tab) => !wereAlreadyOpen.includes(tab));
      if (left.length === 0) break;
      for (const name of left) await endChat(`End chat ${name}`);
      await browser.pause(500);
    }
    await browser.waitUntil(
      async () => (await tabNames()).every((tab) => wereAlreadyOpen.includes(tab)),
      { timeout: 20_000, timeoutMsg: "the activity spec left a tab open behind it" },
    );
  });

  it("sets a task's lines in under the chat that asked, one step a level", async () => {
    const drawn = await measured();
    const at = (depth: number) => drawn.lines.filter((one) => one.depth === depth);
    expect(drawn.lines.map((one) => one.depth)).toEqual([1, 2, 2, 1, 2, 1]);

    // Every line at one depth starts in one place, and a level deeper starts a step in.
    const ones = new Set(at(1).map((one) => Math.round(one.at)));
    const twos = new Set(at(2).map((one) => Math.round(one.at)));
    expect(ones.size).toBe(1);
    expect(twos.size).toBe(1);
    const step = [...twos][0] - [...ones][0];
    expect(step).toBeGreaterThanOrEqual(drawn.rem);
    expect(step).toBeLessThanOrEqual(drawn.rem * 1.5);

    // Nothing is pushed past the timeline's edge, and nothing scrolls sideways.
    for (const one of drawn.lines) expect(one.right).toBeLessThanOrEqual(drawn.list.right + 1);
    expect(drawn.sideways).toBeLessThanOrEqual(0);
    expect(drawn.page).toBeLessThanOrEqual(0);
  });

  it("opens a clipped line in full inside its column, and closes it again", async () => {
    const before = await clippedLine();
    if (before === null) throw new Error("no line was clipped");
    expect(before.says).toBe("Show all");
    expect(before.expanded).toBe("false");
    expect(before.text.endsWith("…")).toBe(true);
    expect(before.text).not.toContain("Checked the backups.");

    await browser.execute(() => {
      document.querySelector<HTMLElement>("ol.activity .activity-more")?.click();
    });
    await browser.waitUntil(async () => (await clippedLine())?.expanded === "true", {
      timeout: 10_000,
      interval: 100,
      timeoutMsg: "Show all never opened the line",
    });

    const opened = await clippedLine();
    if (opened === null) throw new Error("the opened line is gone");
    expect(opened.says).toBe("Show less");
    expect(opened.text).toBe(LONG);
    expect(opened.height).toBeGreaterThan(before.height);
    // The long word is broken inside the column: the line, the list and the page stay as wide.
    const drawn = await measured();
    for (const one of drawn.lines) {
      expect(one.right).toBeLessThanOrEqual(drawn.list.right + 1);
      if (!Number.isNaN(one.textRight))
        expect(one.textRight).toBeLessThanOrEqual(drawn.list.right + 1);
    }
    expect(drawn.sideways).toBeLessThanOrEqual(0);
    expect(drawn.page).toBeLessThanOrEqual(0);

    await browser.execute(() => {
      document.querySelector<HTMLElement>("ol.activity .activity-more")?.click();
    });
    await browser.waitUntil(async () => (await clippedLine())?.expanded === "false", {
      timeout: 10_000,
      interval: 100,
      timeoutMsg: "Show less never closed the line",
    });
    const closed = await clippedLine();
    expect(Math.abs((closed?.height ?? 0) - before.height)).toBeLessThanOrEqual(1);
  });

  it("marks a file two tasks' reports name, its sign in the waiting colour and its words readable", async () => {
    const drawn = await marks();
    expect(drawn.map((one) => one.says)).toEqual([
      "lint's report also names src/app.rs",
      "dig's report also names src/app.rs",
    ]);
    for (const one of drawn) {
      expect(one.sign.width).toBeGreaterThan(4);
      expect(one.sign.height).toBeGreaterThan(4);
      expect(one.signIsWaiting).toBe(true);
      // A mark is held to 3:1, and the sentence, which is prose, to 4.5:1.
      expect(one.signContrast).toBeGreaterThanOrEqual(3);
      expect(one.wordsContrast).toBeGreaterThanOrEqual(4.5);
    }
  });
});
