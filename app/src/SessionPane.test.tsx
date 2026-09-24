import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
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
