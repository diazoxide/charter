import { sizeKey } from "./textSize";

/**
 * The keys for the chats inside a tab (#1487, V100-33, V100-36):
 *
 * | | On a Mac | Everywhere else |
 * |---|---|---|
 * | The menu of this tab's chats | ⌘⇧J | Ctrl+Shift+J |
 * | The next / previous chat in this tab | ⌘⌥↓ / ⌘⌥↑ | Ctrl+Shift+] / Ctrl+Shift+[ |
 * | Back to the session's own chat | ⌘⇧H | Ctrl+Shift+H |
 *
 * **Why these.** A tab's chats hang under it: the menu lists them top to bottom and Down on a
 * tab opens it, so on a Mac the next and the previous chat are Down and Up. Left and right are
 * the strip's. **⌘⇧] and ⌘⇧[ are left free**: they are next and previous TAB in every tabbed
 * program on a Mac, and they are kept for moving along the strip, which nothing does yet. Off
 * a Mac there is no bracket convention to collide with (tabs there are Ctrl+PageUp and
 * Ctrl+PageDown) and Ctrl+Alt with an arrow is the desktop's, so the brackets are the pair. H
 * is "home", as Finder spells Go › Home; J is the free letter beside it. ⌘H alone stays the
 * system's Hide: Shift is part of that chord.
 *
 * **They take nothing from a chat**, by the rule `docs/ui-primitives.md` holds every claimed
 * key to, read in the pinned `@xterm/xterm` 6.0.0 (`common/input/Keyboard.ts`):
 * - a `⌘` chord with a letter sends nothing but `⌘A`;
 * - each arrow's case breaks on `metaKey` before it builds anything, so `⌘⌥↓` and `⌘⌥↑` send
 *   nothing (without ⌘, `⌥↓` is `ESC [1;3B` and stays the terminal's);
 * - `Ctrl` with a letter, `[` or `]` is encoded only when Shift is NOT held, and with Shift
 *   only `_` and `@` are turned into anything.
 * So plain Ctrl+J (newline), Ctrl+H (backspace), Ctrl+[ (Escape) and Ctrl+] still reach the
 * shell. A Mac's `Ctrl` chords stay the terminal's whatever is held with them.
 */
export type TaskKey = "menu" | "next" | "previous" | "own";

/**
 * Which of the four this keystroke is, if any.
 *
 * **By what the key types, not by where it is.** A letter is matched by `key` alone, so a
 * layout that puts another letter on that key fires nothing of this window's twice. A bracket
 * is matched by `key` too; only where the layout has no `[` or `]` without AltGr (German,
 * Nordic, French) is the key in their place taken, and then never when it types a letter or a
 * digit, and never when it is another of the window's own keys: on Dvorak that key types `=`
 * and `+`, which with the same modifier is the text size's.
 */
export function taskKeyOf(e: KeyboardEvent, mac: boolean): TaskKey | undefined {
  if (mac) {
    if (!e.metaKey || e.ctrlKey) return undefined;
    if (e.altKey && !e.shiftKey) {
      if (e.key === "ArrowDown") return "next";
      if (e.key === "ArrowUp") return "previous";
      return undefined;
    }
    if (!e.shiftKey || e.altKey) return undefined;
    return letterOf(e);
  }
  if (!e.ctrlKey || !e.shiftKey || e.metaKey || e.altKey) return undefined;
  const letter = letterOf(e);
  if (letter !== undefined) return letter;
  if (e.key === "]" || e.key === "}") return "next";
  if (e.key === "[" || e.key === "{") return "previous";
  // The key in a bracket's place, on a layout whose brackets need AltGr.
  if (e.code !== "BracketRight" && e.code !== "BracketLeft") return undefined;
  if (sizeKey(e, mac) !== undefined || /^[\p{L}\p{N}]$/u.test(e.key)) return undefined;
  return e.code === "BracketRight" ? "next" : "previous";
}

function letterOf(e: KeyboardEvent): TaskKey | undefined {
  if (e.key === "J" || e.key === "j") return "menu";
  if (e.key === "H" || e.key === "h") return "own";
  return undefined;
}

/** How the rows and the docs spell each key, on this platform. */
export function taskKeySaid(key: TaskKey, mac: boolean): string {
  if (mac) return { menu: "⌘⇧J", next: "⌘⌥↓", previous: "⌘⌥↑", own: "⌘⇧H" }[key];
  return `Ctrl+Shift+${{ menu: "J", next: "]", previous: "[", own: "H" }[key]}`;
}

/**
 * What a row adds about its key where the key is not on every keyboard as written: off a Mac
 * the brackets need AltGr on some layouts, and there the key in their place is the one.
 */
export function taskKeyNote(key: TaskKey, mac: boolean): string {
  if (mac || (key !== "next" && key !== "previous")) return "";
  return ` Where ${key === "next" ? "]" : "["} needs AltGr, it is the key in its place on a US keyboard.`;
}

/** The catalogue's row each key presses. */
export const TASK_KEY_ROW: Readonly<Record<TaskKey, string>> = {
  menu: "tasks.menu",
  next: "tasks.next",
  previous: "tasks.previous",
  own: "tasks.own",
};
