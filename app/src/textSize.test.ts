import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { CHAT_KEYBOARD } from "./actions";
import { forgetThisLaunch } from "./regions";
import {
  DEFAULT_TEXT,
  drawWindowText,
  forgetTextSizes,
  listenForSizeKeys,
  loadText,
  MOST_TEXT,
  LEAST_TEXT,
  onTextSizes,
  resetText,
  setTextSize,
  sizeKey,
  stepText,
  textSizes,
  whoseSize,
} from "./textSize";
import { aboutThisMachine, GLOBAL, sayAboutThisMachine } from "./windowprefs";

const PATH = "/home/op/.config/charter/layout.json";
const REGIONS = [{ id: "explorer", side: "right", order: 0, collapsed: false }];

/** Hands the window a layout file holding `document`, as the initialization script does. */
const put = (document: unknown) => {
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: PATH, found: true, document, trouble: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
};

/** Every layout the window asked the core to keep, parsed. */
let kept: Record<string, unknown>[] = [];

beforeEach(() => {
  Reflect.deleteProperty(globalThis, GLOBAL);
  forgetThisLaunch();
  forgetTextSizes();
  sayAboutThisMachine("text", undefined);
  kept = [];
  mockIPC((cmd, args) => {
    if (cmd === "write_layout") kept.push(JSON.parse((args as { text: string }).text));
    return null;
  });
});
afterEach(() => {
  clearMocks();
  document.body.replaceChildren();
  document.documentElement.style.fontSize = "";
});

describe("the text sizes a machine has never been told about", () => {
  it("are 14px for the window and 13px for the terminal, a step bigger than both were", () => {
    expect(DEFAULT_TEXT).toEqual({ window: 14, terminal: 13 });
    expect(textSizes()).toEqual(DEFAULT_TEXT);
  });

  it("are bounded at 10 and 24", () => {
    expect([LEAST_TEXT, MOST_TEXT]).toEqual([10, 24]);
  });
});

describe("the text sizes kept in the layout file", () => {
  it("are what the file says, beside the regions", () => {
    put({ version: 1, regions: REGIONS, text: { window: 17, terminal: 11 } });

    expect(textSizes()).toEqual({ window: 17, terminal: 11 });
  });

  it("fall back one by one, and say what they put right", () => {
    const { sizes, said } = loadText({
      version: 1,
      regions: [],
      text: { window: 400, terminal: "big" },
    });

    expect(sizes).toEqual(DEFAULT_TEXT);
    expect(said.join("; ")).toContain("window text size 400 is not a whole number from 10 to 24");
    expect(said.join("; ")).toContain('terminal text size "big"');
  });

  it("are the defaults, with nothing said, when the file has none — an older charter's", () => {
    expect(loadText({ version: 1, regions: [] })).toEqual({ sizes: DEFAULT_TEXT, said: [] });
  });

  it("put what they had to put right in the alerts drawer, with the file's path", () => {
    put({ version: 1, regions: [], text: { window: 2.5 } });

    textSizes();

    const [row] = aboutThisMachine();
    expect(row.subject).toBe("text");
    expect(row.detail).toContain(PATH);
    expect(row.detail).toContain("2.5");
  });
});

describe("changing a text size", () => {
  it("is 1px a step, and never past the bounds", () => {
    stepText("window", 1);
    expect(textSizes().window).toBe(15);

    for (let i = 0; i < 30; i++) stepText("window", 1);
    expect(textSizes().window).toBe(MOST_TEXT);

    for (let i = 0; i < 30; i++) stepText("terminal", -1);
    expect(textSizes().terminal).toBe(LEAST_TEXT);
  });

  it("resets to the default, one size at a time", () => {
    setTextSize("window", 20);
    setTextSize("terminal", 20);

    resetText("terminal");

    expect(textSizes()).toEqual({ window: 20, terminal: DEFAULT_TEXT.terminal });
  });

  it("takes a size typed into a box as a whole number within the bounds", () => {
    setTextSize("window", 16.6);
    expect(textSizes().window).toBe(17);
    setTextSize("window", 3);
    expect(textSizes().window).toBe(LEAST_TEXT);
    setTextSize("window", Number.NaN);
    expect(textSizes().window).toBe(LEAST_TEXT);
  });

  it("tells whoever is drawing from it, at once, and only when something changed", () => {
    const heard: number[] = [];
    const stop = onTextSizes((sizes) => heard.push(sizes.terminal));

    stepText("terminal", 1);
    setTextSize("terminal", 14);
    stop();
    stepText("terminal", 1);

    expect(heard).toEqual([14]);
  });

  it("is kept in the layout file, beside the arrangement it does not disturb", async () => {
    put({ version: 1, regions: REGIONS });

    stepText("terminal", 1);
    await vi.waitFor(() => expect(kept).toHaveLength(1));

    const last = kept.at(-1);
    expect(last?.text).toEqual({ window: 14, terminal: 14 });
    expect(last?.version).toBe(1);
    const explorer = (last?.regions as { id: string; side: string }[]).find(
      (one) => one.id === "explorer",
    );
    expect(explorer?.side).toBe("right");
  });

  it("is written once the sizes settle, not once per step of a drag", async () => {
    for (let px = 11; px <= 24; px++) setTextSize("window", px);

    await vi.waitFor(() => expect(kept).toHaveLength(1));
    await new Promise((done) => setTimeout(done, 400));
    expect(kept).toHaveLength(1);
    expect(kept[0].text).toEqual({ window: 24, terminal: 13 });
  });
});

describe("the window's text size", () => {
  it("is the root's font size, so everything drawn in rem scales with it", () => {
    drawWindowText(18);

    expect(document.documentElement.style.fontSize).toBe("18px");
  });
});

describe("the keys", () => {
  const press = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

  it("are ⌘ on a Mac with =, + (bigger), - (smaller) and 0 (reset)", () => {
    expect(sizeKey(press({ key: "=", metaKey: true }), true)).toBe("bigger");
    expect(sizeKey(press({ key: "+", metaKey: true, shiftKey: true }), true)).toBe("bigger");
    expect(sizeKey(press({ key: "-", metaKey: true }), true)).toBe("smaller");
    expect(sizeKey(press({ key: "0", metaKey: true }), true)).toBe("reset");
  });

  it("are Ctrl everywhere else", () => {
    expect(sizeKey(press({ key: "=", ctrlKey: true }), false)).toBe("bigger");
    expect(sizeKey(press({ key: "-", ctrlKey: true }), false)).toBe("smaller");
    expect(sizeKey(press({ key: "0", ctrlKey: true }), false)).toBe("reset");
  });

  it("leave a Mac's Ctrl chords and everyone else's ⌘ chords alone", () => {
    expect(sizeKey(press({ key: "-", ctrlKey: true }), true)).toBeUndefined();
    expect(sizeKey(press({ key: "=", metaKey: true }), false)).toBeUndefined();
  });

  it("are not Ctrl+Shift+- (Ctrl+_), which xterm.js sends as ^_, readline's undo", () => {
    expect(sizeKey(press({ key: "_", ctrlKey: true, shiftKey: true }), false)).toBeUndefined();
    expect(sizeKey(press({ key: "-", ctrlKey: true, shiftKey: true }), false)).toBeUndefined();
  });

  it("are nothing without the modifier, with both, or with Alt", () => {
    expect(sizeKey(press({ key: "=" }), true)).toBeUndefined();
    expect(sizeKey(press({ key: "=", metaKey: true, ctrlKey: true }), true)).toBeUndefined();
    expect(sizeKey(press({ key: "=", metaKey: true, altKey: true }), true)).toBeUndefined();
    expect(sizeKey(press({ key: "9", metaKey: true }), true)).toBeUndefined();
  });
});

describe("which size a key changes", () => {
  /** A pane, marked as a chat's keyboard, holding a textarea as xterm's does. */
  const aPane = () => {
    const pane = document.createElement("div");
    pane.setAttribute(CHAT_KEYBOARD, "");
    const textarea = document.createElement("textarea");
    pane.append(textarea);
    document.body.append(pane);
    return textarea;
  };
  const aButton = () => {
    const button = document.createElement("button");
    document.body.append(button);
    return button;
  };

  it("is the terminal's in a terminal pane, and the window's anywhere else", () => {
    expect(whoseSize(aPane())).toBe("terminal");
    expect(whoseSize(aButton())).toBe("window");
    expect(whoseSize(null)).toBe("window");
  });

  it("follows the focus: a key in a pane changes the terminal, elsewhere the window", () => {
    // jsdom's platform is not a Mac's, so the modifier is Ctrl.
    const stop = listenForSizeKeys(window);
    const inPane = aPane();
    const elsewhere = aButton();

    inPane.dispatchEvent(new KeyboardEvent("keydown", { key: "=", ctrlKey: true, bubbles: true }));
    elsewhere.dispatchEvent(
      new KeyboardEvent("keydown", { key: "-", ctrlKey: true, bubbles: true }),
    );
    stop();

    expect(textSizes()).toEqual({ window: 13, terminal: 14 });
  });

  it("resets only the size the focus is in", () => {
    setTextSize("window", 20);
    setTextSize("terminal", 20);
    const stop = listenForSizeKeys(window);

    aPane().dispatchEvent(new KeyboardEvent("keydown", { key: "0", ctrlKey: true, bubbles: true }));
    stop();

    expect(textSizes()).toEqual({ window: 20, terminal: DEFAULT_TEXT.terminal });
  });

  it("claims its keys before the terminal sees them, and leaves Ctrl+_ to the shell", () => {
    const stop = listenForSizeKeys(window);
    const inPane = aPane();
    const reached: string[] = [];
    inPane.addEventListener("keydown", (e) => reached.push(e.key));

    const bigger = new KeyboardEvent("keydown", {
      key: "=",
      ctrlKey: true,
      bubbles: true,
      cancelable: true,
    });
    const undo = new KeyboardEvent("keydown", {
      key: "_",
      ctrlKey: true,
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    });
    inPane.dispatchEvent(bigger);
    inPane.dispatchEvent(undo);
    stop();

    expect(bigger.defaultPrevented).toBe(true);
    expect(undo.defaultPrevented).toBe(false);
    expect(reached).toEqual(["_"]);
    expect(textSizes().terminal).toBe(DEFAULT_TEXT.terminal + 1);
  });
});
