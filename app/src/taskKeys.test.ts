import { describe, expect, it } from "vitest";
import { TASK_KEY_ROW, taskKeyOf, taskKeySaid, type TaskKey } from "./taskKeys";

const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

describe("the keys for the chats inside a tab", () => {
  it("are ⌘⇧ with J, ], [ and H on a Mac", () => {
    const held = { metaKey: true, shiftKey: true };
    expect(taskKeyOf(key({ key: "J", ...held }), true)).toBe("menu");
    expect(taskKeyOf(key({ key: "}", code: "BracketRight", ...held }), true)).toBe("next");
    expect(taskKeyOf(key({ key: "{", code: "BracketLeft", ...held }), true)).toBe("previous");
    expect(taskKeyOf(key({ key: "h", ...held }), true)).toBe("own");
  });

  it("are Ctrl+Shift with the same keys everywhere else", () => {
    const held = { ctrlKey: true, shiftKey: true };
    expect(taskKeyOf(key({ key: "j", ...held }), false)).toBe("menu");
    expect(taskKeyOf(key({ key: "]", ...held }), false)).toBe("next");
    expect(taskKeyOf(key({ key: "[", ...held }), false)).toBe("previous");
    expect(taskKeyOf(key({ key: "H", ...held }), false)).toBe("own");
  });

  it("reads a bracket by where its key is, whatever Shift types on that layout", () => {
    const held = { ctrlKey: true, shiftKey: true };
    expect(taskKeyOf(key({ key: "*", code: "BracketRight", ...held }), false)).toBe("next");
    expect(taskKeyOf(key({ key: "Ü", code: "BracketLeft", ...held }), false)).toBe("previous");
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
      // Alt is the terminal's Meta, and never one of these.
      expect(
        taskKeyOf(key({ key: k, ctrlKey: true, shiftKey: true, altKey: true }), false),
      ).toBeUndefined();
    }
  });

  it("leaves the system's own ⌘H and ⌘J alone: Shift is part of each", () => {
    expect(taskKeyOf(key({ key: "h", metaKey: true }), true)).toBeUndefined();
    expect(taskKeyOf(key({ key: "j", metaKey: true }), true)).toBeUndefined();
  });

  it("says each key as its platform spells it, and names the row it presses", () => {
    expect(taskKeySaid("menu", true)).toBe("⌘⇧J");
    expect(taskKeySaid("next", false)).toBe("Ctrl+Shift+]");
    expect(taskKeySaid("previous", true)).toBe("⌘⇧[");
    expect(taskKeySaid("own", false)).toBe("Ctrl+Shift+H");
    const keys: TaskKey[] = ["menu", "next", "previous", "own"];
    expect(keys.map((one) => TASK_KEY_ROW[one])).toEqual([
      "tasks.menu",
      "tasks.next",
      "tasks.previous",
      "tasks.own",
    ]);
  });
});
