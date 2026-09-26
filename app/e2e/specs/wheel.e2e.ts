import { browser, expect } from "@wdio/globals";
import { READY } from "../harness.js";
import { endChat, pressAndStart } from "../opening.js";

/**
 * **A trackpad scrolls a chat's terminal by the distance it moved, in the real window** (SI-4).
 *
 * `SessionPane.wheel.test.tsx` holds the arithmetic against the real xterm in jsdom, with a
 * cell size it has to make up: jsdom lays nothing out. What only the real window can say is
 * that the cell size the pane scrolls by is the one WebKit laid out, and that WebKit's wheel
 * events reach the pane's listener before xterm's own viewport, which reads them its own way.
 * So the same slow, many-small-events swipe is sent here, as WebKit wheel events, and what
 * moved is read back from the terminal and from what the program was sent.
 *
 * Measured with these events on `main` before SI-4's fix, row 16px: the history moved 4 rows
 * for the slow 100px and 21 for the 800px flick (6 and 50 now); the program was sent 1 report
 * and 10 (6 and 50 now).
 *
 * A row's height is read from a drawn row, not from the size the pane scrolls by, so the
 * measurement does not agree with itself by construction.
 */

/** The first pane's terminal, as the benchmark hook reads it. */
async function thePane(): Promise<{ scrolledTo: number; mouseTracking: string; session: number }> {
  return browser.execute(() => {
    const pane = window.charterBench?.panes()[0];
    if (!pane) throw new Error("no pane on screen");
    return pane;
  });
}

/** A drawn row's height, in CSS pixels. */
async function rowHeight(): Promise<number> {
  return browser.execute(
    () =>
      document.querySelector(".xterm-rows")?.firstElementChild?.getBoundingClientRect().height ??
      Number.NaN,
  );
}

/** Sends `deltas` as pixel wheel events to the middle of the pane's screen, one after another. */
async function swipe(deltas: number[]): Promise<void> {
  await browser.execute((all: number[]) => {
    const screen = document.querySelector(".xterm-screen");
    if (!screen) throw new Error("no terminal screen");
    const box = screen.getBoundingClientRect();
    for (const deltaY of all) {
      screen.dispatchEvent(
        new WheelEvent("wheel", {
          deltaY,
          deltaMode: WheelEvent.DOM_DELTA_PIXEL,
          clientX: box.left + box.width / 2,
          clientY: box.top + box.height / 2,
          bubbles: true,
          cancelable: true,
        }),
      );
    }
  }, deltas);
}

/** Types into the chat's program, the way a key press does. */
async function type(text: string): Promise<void> {
  await browser.execute((typed: string) => {
    const bench = window.charterBench;
    const session = bench?.panes()[0]?.session;
    if (!bench || session === undefined) throw new Error("no pane to type into");
    bench.type(session, typed);
  }, text);
}

/**
 * How many wheel-down reports the program has been sent. The program's terminal echoes what
 * it is sent, a report's `ESC` as `^[`, so they are counted on the screen: what was sent is
 * what reached the program, not what the page meant to send.
 */
async function reportsEchoed(): Promise<number> {
  const text = await browser.execute(
    () => document.querySelector(".xterm-rows")?.textContent ?? "",
  );
  return text.split("[<65;").length - 1;
}

/** The echoed count once it has stopped changing: the reports travel to the program and back. */
async function settled(): Promise<number> {
  let last = -1;
  let now = await reportsEchoed();
  while (now !== last) {
    await browser.pause(400);
    last = now;
    now = await reportsEchoed();
  }
  return now;
}

describe("the wheel in a chat's terminal", () => {
  let name = "";

  before(async () => {
    await pressAndStart("New tab");
    await browser.waitUntil(
      async () =>
        (
          await browser.execute(() => document.querySelector(".xterm-rows")?.textContent ?? "")
        ).includes(READY),
      { timeout: 30_000, timeoutMsg: "the chat never became ready" },
    );
    name = await browser.execute(
      () =>
        document.querySelector('[role="tab"][aria-selected="true"] .tab-name')?.textContent ?? "",
    );
  });

  after(async () => {
    if (name) await endChat(`End chat ${name}`);
  });

  it("scrolls the history one row per row of pixels, from the first small event", async () => {
    let lines = "";
    for (let i = 1; i <= 120; i++) lines += `line ${i}\r`;
    await type(lines);
    await browser.waitUntil(
      async () =>
        (
          await browser.execute(() => document.querySelector(".xterm-rows")?.textContent ?? "")
        ).includes("you said: line 120"),
      { timeout: 30_000, timeoutMsg: "the program never answered the last line" },
    );
    const row = await rowHeight();
    const bottom = (await thePane()).scrolledTo;

    // A slow start: forty events of one to four pixels, a hundred pixels in all.
    const slow = Array.from({ length: 40 }, (_, i) => -(1 + (i % 4)));
    await swipe(slow);
    const afterSlow = (await thePane()).scrolledTo;

    // A flick of ten events of eighty pixels.
    await swipe(Array.from({ length: 10 }, () => -80));
    const afterFlick = (await thePane()).scrolledTo;

    console.log(
      `[wheel.e2e] row ${row}px; slow 100px → ${bottom - afterSlow} rows; flick 800px → ${afterSlow - afterFlick} rows`,
    );
    expect(bottom - afterSlow).toBe(Math.floor(100 / row));
    expect(afterSlow - afterFlick).toBe(Math.floor((100 + 800) / row) - Math.floor(100 / row));
  });

  it("sends a program that tracks the mouse one report per row of pixels", async () => {
    // The program echoes what it is typed, escapes and all, which is how it asks for mouse
    // tracking here: the alternate screen and SGR wheel reports, as Claude Code's full screen.
    await type("\x1b[?1049h\x1b[?1000h\x1b[?1006h\r");
    await browser.waitUntil(async () => (await thePane()).mouseTracking === "vt200", {
      timeout: 20_000,
      timeoutMsg: "the program never turned mouse tracking on",
    });
    const row = await rowHeight();

    const slow = Array.from({ length: 40 }, (_, i) => 1 + (i % 4));
    await swipe(slow);
    const afterSlow = await settled();
    await swipe(Array.from({ length: 10 }, () => 80));
    const all = await settled();

    console.log(
      `[wheel.e2e] row ${row}px; slow 100px → ${afterSlow} reports; flick 800px → ${all - afterSlow} reports`,
    );
    expect(afterSlow).toBe(Math.floor(100 / row));
    expect(all).toBe(Math.floor(900 / row));
  });
});
