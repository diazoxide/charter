import { describe, expect, it } from "vitest";
import { opensAShell } from "./shellKey";

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

describe("the key that opens a shell tab", () => {
  it("is ⌘⇧T on a Mac", () => {
    expect(opensAShell(press("T", { meta: true, shift: true }), true)).toBe(true);
    expect(opensAShell(press("t", { meta: true, shift: true }), true)).toBe(true);
  });

  it("is Ctrl+Shift+T everywhere else, as in every Linux and Windows terminal", () => {
    expect(opensAShell(press("T", { ctrl: true, shift: true }), false)).toBe(true);
  });

  it("leaves Ctrl+T to the shell, where it transposes two characters", () => {
    expect(opensAShell(press("t", { ctrl: true }), false)).toBe(false);
    expect(opensAShell(press("t", { ctrl: true }), true)).toBe(false);
  });

  it("leaves a Mac's Ctrl chords to the terminal, and another platform's ⌘ alone", () => {
    expect(opensAShell(press("T", { ctrl: true, shift: true }), true)).toBe(false);
    expect(opensAShell(press("T", { meta: true, shift: true }), false)).toBe(false);
  });

  it("is not the key with Alt held too", () => {
    expect(opensAShell(press("T", { meta: true, shift: true, alt: true }), true)).toBe(false);
  });
});
