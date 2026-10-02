import { describe, expect, it } from "vitest";
import { opensTheSwitcher, switcherKeySaid } from "./switcherKey";

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

describe("the key that opens the project switcher (FR-27)", () => {
  it("is ⌘P on a Mac", () => {
    expect(opensTheSwitcher(press("p", { meta: true }), true)).toBe(true);
    expect(switcherKeySaid(true)).toBe("⌘P");
  });

  it("is Ctrl+Shift+P everywhere else", () => {
    expect(opensTheSwitcher(press("P", { ctrl: true, shift: true }), false)).toBe(true);
    expect(switcherKeySaid(false)).toBe("Ctrl+Shift+P");
  });

  it("leaves Ctrl+P to the shell, where it is the previous line of history", () => {
    expect(opensTheSwitcher(press("p", { ctrl: true }), false)).toBe(false);
    expect(opensTheSwitcher(press("p", { ctrl: true }), true)).toBe(false);
  });

  it("leaves a Mac's Ctrl chords to the terminal, and another platform's ⌘ alone", () => {
    expect(opensTheSwitcher(press("P", { ctrl: true, shift: true }), true)).toBe(false);
    expect(opensTheSwitcher(press("p", { meta: true }), false)).toBe(false);
  });

  it("is not ⌘⇧P, nor the key with Alt held too", () => {
    expect(opensTheSwitcher(press("P", { meta: true, shift: true }), true)).toBe(false);
    expect(opensTheSwitcher(press("p", { meta: true, alt: true }), true)).toBe(false);
  });
});
