import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { FitAddon } from "@xterm/addon-fit";
import type { Terminal } from "@xterm/xterm";
import { SessionPane } from "./SessionPane";
import { DEFAULT_TEXT, forgetTextSizes, setTextSize } from "./textSize";
import { BUILT_IN, DEFAULT_THEME, drawIn, xtermTheme } from "./theme/theme";

/**
 * **The terminal follows a theme switched while it is up** (M6.7).
 *
 * The real xterm, not a stand-in: what is asserted is the terminal's own `options.theme`, which
 * is what xterm draws from. The class is only wrapped so the test can reach the instance the
 * pane made.
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

// The renderer loads WebGL code jsdom has no context for; the pane draws with the DOM meanwhile.
vi.mock("./renderer", async (real) => ({
  ...(await real<typeof import("./renderer")>()),
  draw: () => new Promise(() => {}),
}));

// jsdom has no `matchMedia`, and xterm asks it for the display's pixel ratio as it opens.
vi.stubGlobal("matchMedia", (query: string) => ({
  matches: false,
  media: query,
  addEventListener: () => {},
  removeEventListener: () => {},
  addListener: () => {},
  removeListener: () => {},
}));

const PLANE = "/planes/one";
const LIGHT = BUILT_IN["charter-light"];

beforeEach(() => {
  made.length = 0;
  forgetTextSizes();
  drawIn(DEFAULT_THEME);
  // The view never opens: the theme is decided before any output could arrive.
  mockIPC(() => new Promise(() => {}));
});
afterEach(() => {
  cleanup();
  clearMocks();
  drawIn(DEFAULT_THEME);
});

const pane = () =>
  render(<SessionPane plane={PLANE} session={1} focused={false} onFocus={() => {}} />);

describe("a terminal and the theme in force", () => {
  it("is built in the theme in force", () => {
    pane();

    expect(made).toHaveLength(1);
    expect(made[0].options.theme).toEqual(xtermTheme(DEFAULT_THEME));
  });

  it("is handed the new theme when one is drawn while it is up", () => {
    pane();
    expect(made[0].options.theme?.background).not.toBe(LIGHT.values["terminal.background"]);

    act(() => drawIn(LIGHT));

    expect(made[0].options.theme).toEqual(xtermTheme(LIGHT));
  });

  it("stops following once the pane is gone", () => {
    const { unmount } = pane();
    const was = made[0];
    unmount();

    // A disposed terminal handed a theme throws in xterm; this is the pane having let go.
    expect(() => drawIn(LIGHT)).not.toThrow();
    expect(was.options.theme).toEqual(xtermTheme(DEFAULT_THEME));
  });
});

describe("a terminal and the terminal text size (charter-app#283)", () => {
  it("is built at the terminal size in force, one step bigger than the 12 it was", () => {
    pane();

    expect(DEFAULT_TEXT.terminal).toBe(13);
    expect(made[0].options.fontSize).toBe(13);
  });

  it("takes a new size while it is up, and refits its rows and columns to it", () => {
    pane();
    const fit = vi.spyOn(FitAddon.prototype, "fit");

    act(() => setTextSize("terminal", 17));

    expect(made[0].options.fontSize).toBe(17);
    // After the size, not before: the grid is measured in the new cells.
    expect(fit).toHaveBeenCalled();
    fit.mockRestore();
  });

  it("is not told about the window's size", () => {
    pane();

    act(() => setTextSize("window", 20));

    expect(made[0].options.fontSize).toBe(13);
  });

  it("stops following once the pane is gone", () => {
    const { unmount } = pane();
    const was = made[0];
    unmount();

    expect(() => setTextSize("terminal", 19)).not.toThrow();
    expect(was.options.fontSize).toBe(13);
  });
});

/** A key pressed in the pane's terminal: delivered where xterm reads the keyboard, its own
 *  textarea, exactly as WebKit delivers one. Answers the event, so what became of its default
 *  can be asked. */
function press(init: KeyboardEventInit & { keyCode?: number }, on?: Element | null): KeyboardEvent {
  const target = on ?? document.querySelector(".xterm-helper-textarea");
  if (!target) throw new Error("the pane has no terminal to type into");
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  // jsdom's KeyboardEvent has no `keyCode` of its own, and xterm encodes keys by it.
  if (init.keyCode !== undefined) Object.defineProperty(event, "keyCode", { value: init.keyCode });
  act(() => {
    target.dispatchEvent(event);
  });
  return event;
}

/** What the pane handed the program, in order: every `send_input` the core was asked for. */
let sent: string[] = [];

/** A view that opens, on a session running `newline`'s harness — or a shell, for null. */
function opening(newline: string | null) {
  sent = [];
  mockIPC((cmd, args) => {
    if (cmd === "watch_session")
      return { view: 1, columns: 80, rows: 24, scrollback: 5000, newline };
    if (cmd === "send_input") sent.push((args as { text: string }).text);
    return null;
  });
}

/** Renders a pane and waits until its view is open, so the pane knows its harness. */
async function openPane() {
  pane();
  await waitFor(() => expect(made[0].options.scrollback).toBe(5000));
}

const onA = (platform: string) => vi.spyOn(navigator, "platform", "get").mockReturnValue(platform);

describe("Shift+Enter in a chat's terminal (SI-4)", () => {
  it("sends the harness's newline rather than a return, so the input grows a line", async () => {
    opening("\u001b\r");
    await openPane();

    press({ key: "Enter", keyCode: 13, shiftKey: true });

    await vi.waitFor(() => expect(sent).toEqual(["\u001b\r"]));
  });

  it("leaves plain Enter a return, which submits", async () => {
    opening("\u001b\r");
    await openPane();

    press({ key: "Enter", keyCode: 13 });

    await vi.waitFor(() => expect(sent).toEqual(["\r"]));
  });

  it("leaves a shell's Shift+Enter to the terminal, which sends a return", async () => {
    opening(null);
    await openPane();

    press({ key: "Enter", keyCode: 13, shiftKey: true });

    await vi.waitFor(() => expect(sent).toEqual(["\r"]));
  });
});

describe("finding text in a chat's terminal (SI-4)", () => {
  afterEach(() => vi.restoreAllMocks());

  const bar = () => screen.queryByRole("search", { name: "Find in the chat" });
  const field = () => screen.getByRole("searchbox", { name: "Find" });

  it("opens on ⌘F on a Mac, with the keyboard in the find field", async () => {
    onA("MacIntel");
    opening("\u001b\r");
    await openPane();

    const chord = press({ key: "f", keyCode: 70, metaKey: true });

    expect(bar()).not.toBeNull();
    expect(document.activeElement).toBe(field());
    // Neither the page's own find nor the program is given the chord.
    expect(chord.defaultPrevented).toBe(true);
    expect(sent).toEqual([]);
  });

  it("opens on Ctrl+Shift+F off a Mac, and leaves Ctrl+F to the shell as forward-char", async () => {
    onA("Linux x86_64");
    opening(null);
    await openPane();

    const plain = press({ key: "f", keyCode: 70, ctrlKey: true });
    expect(bar()).toBeNull();
    expect(plain.defaultPrevented).toBe(true); // xterm's own: it sent the byte on
    await vi.waitFor(() => expect(sent).toEqual(["\u0006"]));

    press({ key: "F", keyCode: 70, ctrlKey: true, shiftKey: true });
    expect(bar()).not.toBeNull();
    expect(sent).toEqual(["\u0006"]);
  });

  it("closes on Esc and gives the keyboard back to the terminal", async () => {
    onA("MacIntel");
    opening(null);
    await openPane();
    press({ key: "f", keyCode: 70, metaKey: true });

    press({ key: "Escape" }, field());

    expect(bar()).toBeNull();
    expect(document.activeElement).toBe(document.querySelector(".xterm-helper-textarea"));
    expect(sent).toEqual([]);
  });

  it("counts the matches, and walks them with Enter and Shift+Enter", async () => {
    onA("MacIntel");
    opening(null);
    await openPane();
    await new Promise<void>((done) => made[0].write("one fish\r\ntwo fish\r\nred fish\r\n", done));
    press({ key: "f", keyCode: 70, metaKey: true });
    const status = () => screen.getByRole("status").textContent;

    act(() => {
      fireInput(field(), "fish");
    });
    await waitFor(() => expect(status()).toBe("1 of 3"));

    press({ key: "Enter" }, field());
    await waitFor(() => expect(status()).toBe("2 of 3"));

    press({ key: "Enter", shiftKey: true }, field());
    await waitFor(() => expect(status()).toBe("1 of 3"));

    act(() => {
      fireInput(field(), "whale");
    });
    await waitFor(() => expect(status()).toBe("No matches"));
  });
});

/** Types `value` into a text field the way React hears it. */
function fireInput(on: HTMLElement, value: string) {
  const input = on as HTMLInputElement;
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
  set?.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
}
