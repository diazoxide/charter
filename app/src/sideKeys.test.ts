import { describe, expect, it } from "vitest";
import { sideKeyOf, sideKeysSaid } from "./sideKeys";

/** A keydown as the window's capture listener receives it. */
function press(
  key: string,
  held: { meta?: boolean; ctrl?: boolean; shift?: boolean; alt?: boolean },
) {
  return new KeyboardEvent("keydown", {
    key,
    metaKey: held.meta ?? false,
    ctrlKey: held.ctrl ?? false,
    shiftKey: held.shift ?? false,
    altKey: held.alt ?? false,
  });
}

describe("the keys of the left side (#1673, B-10)", () => {
  it("are an editor's on a Mac: ⌘B the side, ⌘⇧E Explorer, ⌘⇧C Chats", () => {
    expect(sideKeyOf(press("b", { meta: true }), true)).toEqual({ toggle: "navigation" });
    expect(sideKeyOf(press("E", { meta: true, shift: true }), true)).toEqual({ show: "explorer" });
    expect(sideKeyOf(press("C", { meta: true, shift: true }), true)).toEqual({ show: "chats" });
  });

  it("take Ctrl in place of ⌘ everywhere else", () => {
    expect(sideKeyOf(press("b", { ctrl: true }), false)).toMatchObject({ toggle: "navigation" });
    expect(sideKeyOf(press("E", { ctrl: true, shift: true }), false)).toMatchObject({
      show: "explorer",
    });
    expect(sideKeyOf(press("C", { ctrl: true, shift: true }), false)).toMatchObject({
      show: "chats",
    });
  });

  it("say whether the chord is one a terminal has a use for, so a chat can keep it", () => {
    // Ctrl+B is a byte (readline's back-char, tmux's prefix); Ctrl+Shift+C is a Linux
    // terminal's copy. Neither is claimed from under a chat off a Mac. A ⌘ chord is no byte.
    expect(sideKeyOf(press("b", { ctrl: true }), false)?.chatKeeps).toBe(true);
    expect(sideKeyOf(press("C", { ctrl: true, shift: true }), false)?.chatKeeps).toBe(true);
    expect(sideKeyOf(press("E", { ctrl: true, shift: true }), false)?.chatKeeps).toBeUndefined();
    expect(sideKeyOf(press("b", { meta: true }), true)?.chatKeeps).toBeUndefined();
  });

  it("are not the keys with another modifier held", () => {
    expect(sideKeyOf(press("b", { meta: true, shift: true }), true)).toBeUndefined();
    expect(sideKeyOf(press("b", { meta: true, alt: true }), true)).toBeUndefined();
    expect(sideKeyOf(press("E", { meta: true }), true)).toBeUndefined();
    expect(sideKeyOf(press("E", { meta: true, shift: true, alt: true }), true)).toBeUndefined();
  });

  it("leave a Mac's Ctrl chords to the terminal, and another platform's ⌘ alone", () => {
    expect(sideKeyOf(press("b", { ctrl: true }), true)).toBeUndefined();
    expect(sideKeyOf(press("b", { meta: true }), false)).toBeUndefined();
  });

  it("are spelled for the platform", () => {
    expect(sideKeysSaid(true)).toEqual({
      navigation: "⌘B",
      chats: "⌘⇧C",
      explorer: "⌘⇧E",
      search: "⌘⇧F",
      changes: "⌃⇧G",
    });
    expect(sideKeysSaid(false)).toEqual({
      navigation: "Ctrl+B",
      chats: "Ctrl+Shift+C",
      explorer: "Ctrl+Shift+E",
      search: "Ctrl+Shift+F",
      changes: "Ctrl+Shift+G",
    });
  });
});

describe("the keys of Search and Changes (#1676, B-10)", () => {
  it("show Search on ⌘⇧F, and Ctrl+Shift+F everywhere else", () => {
    expect(sideKeyOf(press("F", { meta: true, shift: true }), true)).toEqual({ show: "search" });
    expect(sideKeyOf(press("f", { ctrl: true, shift: true }), false)).toMatchObject({
      show: "search",
    });
  });

  it("leave Ctrl+Shift+F to a chat off a Mac, where it is the chat's find bar", () => {
    expect(sideKeyOf(press("F", { ctrl: true, shift: true }), false)?.chatKeeps).toBe(true);
    expect(sideKeyOf(press("F", { meta: true, shift: true }), true)?.chatKeeps).toBeUndefined();
  });

  it("show Changes on Ctrl+Shift+G on every platform, a Mac's too, as an editor's", () => {
    // ⌃⇧G is no byte: xterm.js encodes Ctrl with a letter only without Shift. So it is the
    // window's even while a chat has the keyboard, on a Mac as everywhere else.
    expect(sideKeyOf(press("G", { ctrl: true, shift: true }), true)).toEqual({ show: "changes" });
    expect(sideKeyOf(press("g", { ctrl: true, shift: true }), false)).toEqual({ show: "changes" });
  });

  it("are not Changes with ⌘ on a Mac, nor without Shift", () => {
    expect(sideKeyOf(press("G", { meta: true, shift: true }), true)).toBeUndefined();
    expect(sideKeyOf(press("g", { ctrl: true }), true)).toBeUndefined();
    expect(sideKeyOf(press("g", { ctrl: true }), false)).toBeUndefined();
    expect(sideKeyOf(press("G", { ctrl: true, shift: true, meta: true }), true)).toBeUndefined();
  });
});
