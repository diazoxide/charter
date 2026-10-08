import { describe, expect, it } from "vitest";
import { sizeKey } from "./textSize";
import { TASK_KEY_ROW, taskKeyNote, taskKeyOf, taskKeySaid, type TaskKey } from "./taskKeys";

const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

describe("the keys for the chats inside a tab", () => {
  it("are ⌘⇧J and ⌘⇧H, and ⌘⌥ with Down and Up, on a Mac", () => {
    const shift = { metaKey: true, shiftKey: true };
    const option = { metaKey: true, altKey: true };
    expect(taskKeyOf(key({ key: "J", ...shift }), true)).toBe("menu");
    expect(taskKeyOf(key({ key: "h", ...shift }), true)).toBe("own");
    expect(taskKeyOf(key({ key: "ArrowDown", ...option }), true)).toBe("next");
    expect(taskKeyOf(key({ key: "ArrowUp", ...option }), true)).toBe("previous");
  });

  it("leaves ⌘⇧] and ⌘⇧[ free on a Mac: they are the strip's", () => {
    const held = { metaKey: true, shiftKey: true };
    expect(taskKeyOf(key({ key: "}", code: "BracketRight", ...held }), true)).toBeUndefined();
    expect(taskKeyOf(key({ key: "{", code: "BracketLeft", ...held }), true)).toBeUndefined();
  });

  it("are Ctrl+Shift with J, ], [ and H everywhere else", () => {
    const held = { ctrlKey: true, shiftKey: true };
    expect(taskKeyOf(key({ key: "j", ...held }), false)).toBe("menu");
    expect(taskKeyOf(key({ key: "}", code: "BracketRight", ...held }), false)).toBe("next");
    expect(taskKeyOf(key({ key: "{", code: "BracketLeft", ...held }), false)).toBe("previous");
    expect(taskKeyOf(key({ key: "H", ...held }), false)).toBe("own");
    // And not the Mac's arrows, which are the desktop's there.
    expect(
      taskKeyOf(key({ key: "ArrowDown", ctrlKey: true, altKey: true }), false),
    ).toBeUndefined();
  });

  it("matches a letter by what the key types, never by where it is", () => {
    const held = { ctrlKey: true, shiftKey: true };
    // The key where J is on a US keyboard, on a layout where it types something else.
    expect(taskKeyOf(key({ key: "H", code: "KeyJ", ...held }), false)).toBe("own");
    expect(taskKeyOf(key({ key: "о", code: "KeyJ", ...held }), false)).toBeUndefined();
  });

  it("takes the key in a bracket's place where the layout's brackets need AltGr", () => {
    const held = { ctrlKey: true, shiftKey: true };
    // German: the key right of Ü types + and *, and the one in [ 's place types ü and Ü.
    expect(taskKeyOf(key({ key: "*", code: "BracketRight", ...held }), false)).toBe("next");
    // A letter there is that layout's letter, and is not taken.
    expect(taskKeyOf(key({ key: "Ü", code: "BracketLeft", ...held }), false)).toBeUndefined();
    // French: ^ and ¨ in [ 's place.
    expect(taskKeyOf(key({ key: "¨", code: "BracketLeft", ...held }), false)).toBe("previous");
  });

  it("does not fire with the text size's key on Dvorak, where ] 's place types = and +", () => {
    const zoom = key({ key: "+", code: "BracketRight", ctrlKey: true, shiftKey: true });
    expect(sizeKey(zoom, false)).toBe("bigger");
    expect(taskKeyOf(zoom, false)).toBeUndefined();
    // Dvorak's own ] is elsewhere, and is the key by what it types.
    expect(taskKeyOf(key({ key: "}", code: "Equal", ctrlKey: true, shiftKey: true }), false)).toBe(
      "next",
    );
  });

  it("leaves a chat every chord its terminal turns into bytes", () => {
    // Ctrl with the key and no Shift is the shell's: newline, backspace, Escape, and ^].
    for (const k of ["j", "h", "[", "]"]) {
      expect(taskKeyOf(key({ key: k, ctrlKey: true }), false)).toBeUndefined();
      // On a Mac every Ctrl chord is the terminal's, whatever is held with it.
      expect(taskKeyOf(key({ key: k, ctrlKey: true, shiftKey: true }), true)).toBeUndefined();
      expect(
        taskKeyOf(key({ key: k, ctrlKey: true, metaKey: true, shiftKey: true }), true),
      ).toBeUndefined();
      // Alt with Ctrl+Shift is never one of these.
      expect(
        taskKeyOf(key({ key: k, ctrlKey: true, shiftKey: true, altKey: true }), false),
      ).toBeUndefined();
    }
    // Option with an arrow and no ⌘ is the terminal's own (`ESC [1;3B`).
    expect(taskKeyOf(key({ key: "ArrowDown", altKey: true }), true)).toBeUndefined();
    expect(taskKeyOf(key({ key: "ArrowDown", metaKey: true }), true)).toBeUndefined();
    expect(
      taskKeyOf(key({ key: "ArrowDown", metaKey: true, altKey: true, ctrlKey: true }), true),
    ).toBeUndefined();
  });

  it("leaves the system's own ⌘H and ⌘J alone: Shift is part of each", () => {
    expect(taskKeyOf(key({ key: "h", metaKey: true }), true)).toBeUndefined();
    expect(taskKeyOf(key({ key: "j", metaKey: true }), true)).toBeUndefined();
  });

  it("says each key as its platform spells it, and names the row it presses", () => {
    const keys: TaskKey[] = ["menu", "next", "previous", "own"];
    expect(keys.map((one) => taskKeySaid(one, true))).toEqual(["⌘⇧J", "⌘⌥↓", "⌘⌥↑", "⌘⇧H"]);
    expect(keys.map((one) => taskKeySaid(one, false))).toEqual([
      "Ctrl+Shift+J",
      "Ctrl+Shift+]",
      "Ctrl+Shift+[",
      "Ctrl+Shift+H",
    ]);
    expect(keys.map((one) => TASK_KEY_ROW[one])).toEqual([
      "tasks.menu",
      "tasks.next",
      "tasks.previous",
      "tasks.own",
    ]);
  });

  it("says that a bracket may need AltGr, only where the key is a bracket", () => {
    expect(taskKeyNote("next", false)).toContain("AltGr");
    expect(taskKeyNote("previous", false)).toContain("AltGr");
    expect(taskKeyNote("menu", false)).toBe("");
    expect(taskKeyNote("next", true)).toBe("");
  });
});
