import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { Terminal } from "@xterm/xterm";
import { SessionPane } from "./SessionPane";

/**
 * The real xterm, fed wheel events as a trackpad sends them, and what it did asked of the
 * terminal itself: the reports a program is sent, and the row the history is scrolled to.
 */
const made: Terminal[] = [];
vi.mock("@xterm/xterm", async (real) => {
  const xterm = await real<typeof import("@xterm/xterm")>();
  class Recorded extends xterm.Terminal {
    constructor(...args: ConstructorParameters<typeof xterm.Terminal>) {
      super(...args);
      made.push(this);
    }
  }
  return { ...xterm, Terminal: Recorded };
});
vi.mock("./renderer", async (real) => ({
  ...(await real<typeof import("./renderer")>()),
  draw: () => new Promise(() => {}),
}));
vi.stubGlobal("matchMedia", (query: string) => ({
  matches: false,
  media: query,
  addEventListener: () => {},
  removeEventListener: () => {},
  addListener: () => {},
  removeListener: () => {},
}));

/** A cell 8 by 17 CSS pixels — jsdom lays nothing out, so xterm's own measuring span is
 *  given the size a 13px monospace face has. */
const CELL = 17;
const measuring = (el: HTMLElement) => el.classList.contains("xterm-char-measure-element");

beforeEach(() => {
  made.length = 0;
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return measuring(this) ? 8 * 32 : 0;
  });
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return measuring(this) ? CELL : 0;
  });
  mockIPC(() => new Promise(() => {}));
});
afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
});

/** A trackpad swipe as macOS delivers it: a slow start of a few pixels an event, a flick,
 *  then the momentum tail decaying back to a pixel. Positive is down (content moves up). */
function swipe(): number[] {
  const deltas: number[] = [];
  for (let i = 0; i < 30; i++) deltas.push(1 + (i % 4));
  for (let d = 20; d <= 80; d += 6) deltas.push(d);
  for (let d = 80; d >= 1; d = Math.floor(d * 0.85)) deltas.push(d);
  return deltas;
}

function wheel(on: Element, deltaY: number, deltaMode: number = WheelEvent.DOM_DELTA_PIXEL) {
  on.dispatchEvent(
    new WheelEvent("wheel", {
      deltaY,
      deltaMode,
      clientX: 20,
      clientY: 20,
      bubbles: true,
      cancelable: true,
    }),
  );
}

const write = (term: Terminal, text: string) => new Promise<void>((done) => term.write(text, done));

/** Six hundred lines, so there is history above the screen to scroll through. */
function history(): string {
  let text = "";
  for (let i = 0; i < 600; i++) text += `line ${i}\r\n`;
  return text;
}

async function opened() {
  render(<SessionPane plane="/planes/one" session={1} focused={false} onFocus={() => {}} />);
  const term = made[0];
  const screen = term.element?.querySelector(".xterm-screen");
  if (!screen) throw new Error("the pane opened no terminal screen");
  return { term, screen };
}

/** What a wheel report asks for: `ESC [ < 64 ; …` is up, `65` is down, in SGR. */
const sgr = (reports: string[], button: 64 | 65) =>
  reports.filter((r) => r.startsWith(`\x1b[<${button};`)).length;

const total = (deltas: number[]) => deltas.reduce((sum, d) => sum + d, 0);

/**
 * **A trackpad scrolls a chat's terminal by the distance the fingers moved** (SI-4) — a line,
 * or a wheel report, for every cell's height of pixels, carried across events, as Zed's
 * terminal does. xterm.js 6.0.0 scaled every delta under 50px by 0.3 and sent at most
 * one report an event, so a slow start barely moved and a flick was cut off.
 */
describe("a trackpad in a chat's terminal (SI-4)", () => {
  it("sends a program that tracks the mouse one wheel report per row the fingers moved", async () => {
    const { term, screen } = await opened();
    await write(term, "\x1b[?1049h\x1b[?1000h\x1b[?1002h\x1b[?1006h");
    const reports: string[] = [];
    term.onData((d) => reports.push(d));

    const deltas = swipe();
    for (const d of deltas) wheel(screen, d);

    expect(sgr(reports, 65)).toBe(Math.floor(total(deltas) / CELL));
    expect(sgr(reports, 64)).toBe(0);
  });

  it("answers a slow start as it goes, with no dead zone", async () => {
    const { term, screen } = await opened();
    await write(term, "\x1b[?1049h\x1b[?1000h\x1b[?1006h");
    const reports: string[] = [];
    term.onData((d) => reports.push(d));

    // Thirty events of one to four pixels: 75px, four rows and a bit.
    const slow = swipe().slice(0, 30);
    for (const d of slow) wheel(screen, -d);

    expect(sgr(reports, 64)).toBe(Math.floor(total(slow) / CELL));
  });

  it("answers a flick in full, several reports to an event", async () => {
    const { term, screen } = await opened();
    await write(term, "\x1b[?1049h\x1b[?1000h\x1b[?1006h");
    const reports: string[] = [];
    term.onData((d) => reports.push(d));

    wheel(screen, 80);

    expect(sgr(reports, 65)).toBe(4);
  });

  it("leaves the report's encoding to xterm, which knows the one the program asked for", async () => {
    const { term, screen } = await opened();
    // SGR-pixels (?1016): the pointer at (20, 20) in pixels, where in cells it is (3, 2).
    await write(term, "\x1b[?1049h\x1b[?1000h\x1b[?1016h");
    const reports: string[] = [];
    term.onData((d) => reports.push(d));

    wheel(screen, 2 * CELL);

    expect(reports).toEqual(["\x1b[<65;20;20M", "\x1b[<65;20;20M"]);
  });

  it("scrolls the history one line per row the fingers moved", async () => {
    const { term, screen } = await opened();
    await write(term, history());
    term.scrollToTop();

    const deltas = swipe();
    for (const d of deltas) wheel(screen, d);

    expect(term.buffer.active.viewportY).toBe(Math.floor(total(deltas) / CELL));
  });

  it("scrolls the history back up the same way", async () => {
    const { term, screen } = await opened();
    await write(term, history());
    const bottom = term.buffer.active.viewportY;

    const deltas = swipe();
    for (const d of deltas) wheel(screen, -d);

    expect(bottom - term.buffer.active.viewportY).toBe(Math.floor(total(deltas) / CELL));
  });

  it("turns straight round when the fingers do, with nothing carried over", async () => {
    const { term, screen } = await opened();
    await write(term, history());
    term.scrollToTop();
    wheel(screen, CELL + CELL - 1);
    expect(term.buffer.active.viewportY).toBe(1);

    wheel(screen, -CELL);

    expect(term.buffer.active.viewportY).toBe(0);
  });

  it("sends a full-screen program without mouse tracking an arrow per row", async () => {
    const { term, screen } = await opened();
    await write(term, "\x1b[?1049h");
    const sent: string[] = [];
    term.onData((d) => sent.push(d));

    const deltas = swipe();
    for (const d of deltas) wheel(screen, d);

    expect(sent.join("")).toBe("\x1b[B".repeat(Math.floor(total(deltas) / CELL)));
  });

  it("scrolls a mouse wheel's notch the lines it says, and a page a screenful", async () => {
    const { term, screen } = await opened();
    await write(term, history());
    term.scrollToTop();

    wheel(screen, 3, WheelEvent.DOM_DELTA_LINE);
    expect(term.buffer.active.viewportY).toBe(3);

    wheel(screen, 1, WheelEvent.DOM_DELTA_PAGE);
    expect(term.buffer.active.viewportY).toBe(3 + term.rows);
  });
});
