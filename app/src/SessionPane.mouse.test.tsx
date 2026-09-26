import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { Terminal } from "@xterm/xterm";
import { SessionPane } from "./SessionPane";

/**
 * **A program that asks for mouse reports in the default encoding gets them** (charter#493).
 *
 * The real xterm, and the real IPC mock: what is asserted is the bytes the core is asked to
 * write to the session's pty, whichever command carries them. xterm.js 6.0.0 hands the
 * default (X10-style) encoding — `?1000h` without `?1006h` — to `onBinary`, not `onData`, one
 * character per byte, so a report for a column past 95 has a byte above 127 in it.
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
const WIDTH = 8;
const CELL = 17;
const measuring = (el: HTMLElement) => el.classList.contains("xterm-char-measure-element");

/** Every byte the core was asked to write to the session's pty, in order. */
let written: number[] = [];

beforeEach(() => {
  made.length = 0;
  written = [];
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return measuring(this) ? WIDTH * 32 : 0;
  });
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return measuring(this) ? CELL : 0;
  });
  mockIPC((cmd, args) => {
    if (cmd === "watch_session")
      return { view: 1, columns: 200, rows: 24, scrollback: 5000, newline: null };
    const sent = args as { text?: string; bytes?: number[] };
    if (cmd === "send_input" && sent.text !== undefined)
      written.push(...new TextEncoder().encode(sent.text));
    if (cmd === "send_input_bytes" && sent.bytes !== undefined) written.push(...sent.bytes);
    return null;
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
});

const write = (term: Terminal, text: string) => new Promise<void>((done) => term.write(text, done));

async function opened() {
  render(<SessionPane plane="/planes/one" session={1} focused={false} onFocus={() => {}} />);
  const term = made[0];
  await waitFor(() => expect(term.cols).toBe(200));
  const screen = term.element?.querySelector(".xterm-screen");
  if (!screen) throw new Error("the pane opened no terminal screen");
  return { term, screen };
}

/** The pointer over the cell at (`col`, `row`), zero-based. */
const over = (col: number, row: number) => ({
  clientX: col * WIDTH + WIDTH / 2,
  clientY: row * CELL + CELL / 2,
  bubbles: true,
  cancelable: true,
});

/** A default-encoding report: `CSI M`, then the button, column and row, each plus 32, one-based. */
const report = (button: number, col: number, row: number) => [
  0x1b,
  0x5b,
  0x4d,
  button + 32,
  col + 1 + 32,
  row + 1 + 32,
];

describe("mouse reports in the default encoding (charter#493)", () => {
  it("are what SGR's are not: SGR reaches the program as text already", async () => {
    const { term, screen } = await opened();
    await write(term, "\x1b[?1049h\x1b[?1000h\x1b[?1006h");

    screen.dispatchEvent(new MouseEvent("mousedown", { ...over(10, 3), button: 0 }));

    await vi.waitFor(() => expect(written).toEqual([...new TextEncoder().encode("\x1b[<0;11;4M")]));
  });

  it("reach the program when it clicks", async () => {
    const { term, screen } = await opened();
    await write(term, "\x1b[?1049h\x1b[?1000h");

    screen.dispatchEvent(new MouseEvent("mousedown", { ...over(10, 3), button: 0 }));

    await vi.waitFor(() => expect(written).toEqual(report(0, 10, 3)));
  });

  it("reach the program when it scrolls", async () => {
    const { term, screen } = await opened();
    await write(term, "\x1b[?1049h\x1b[?1000h");

    screen.dispatchEvent(new WheelEvent("wheel", { ...over(10, 3), deltaY: CELL }));

    // 64 + 1: the wheel, down.
    await vi.waitFor(() => expect(written).toEqual(report(65, 10, 3)));
  });

  it("carry a column past 95 as the one byte it is, not re-encoded as text", async () => {
    const { term, screen } = await opened();
    await write(term, "\x1b[?1049h\x1b[?1000h");

    screen.dispatchEvent(new MouseEvent("mousedown", { ...over(150, 3), button: 0 }));

    // Column 151 is the byte 183: sent as UTF-8 text it would be two, 0xC2 0xB7.
    await vi.waitFor(() => expect(written).toEqual(report(0, 150, 3)));
  });
});
