/**
 * The keys for the chats inside a tab (#1487, V100-33, V100-36), **⌘⇧ on a Mac and Ctrl+Shift
 * everywhere else**, with:
 *
 * - **J**: the menu of this tab's chats, its session's and its tasks';
 * - **]** and **[**: the next and the previous chat in this tab, in the menu's order;
 * - **H**: back to the session's own chat, the tab's home.
 *
 * **Why these.** One pattern for all four, the one the new-shell key has (`shellKey.ts`), so
 * they are learned once. `]` and `[` are the pair every tabbed program moves between
 * neighbours with, here between the chats of one tab; H is "home", the way Finder spells Go ›
 * Home; J is the free letter beside it. ⌘H alone is the system's Hide and ⌘J nothing of this
 * window's, and neither is touched: Shift is part of every chord here.
 *
 * **They take nothing from a chat**, by the rule `docs/ui-primitives.md` holds every claimed
 * key to, read against the pinned `@xterm/xterm` 6.0.0 (`common/input/Keyboard.ts`): it sends
 * nothing for a `⌘` chord but `⌘A`, and it encodes `Ctrl` with a letter, `[` or `]` only when
 * Shift is NOT held. So Ctrl+Shift+J, H, `[` and `]` were never bytes, and plain Ctrl+J
 * (newline), Ctrl+H (backspace), Ctrl+[ (Escape) and Ctrl+] still reach the shell. A Mac's
 * `Ctrl` chords stay the terminal's whatever is held with them, and Alt is never one of these.
 * Nothing else in the window had them: the palette has `F2` and `⌘K`/`Ctrl-K`, the text sizes
 * `=`, `-` and `0`, the shell `T`, the switcher `P`, find and search `F`, the menu `Q` and `,`.
 */
export type TaskKey = "menu" | "next" | "previous" | "own";

/** Which of the four this keystroke is, if any. The brackets are read by where the key is as
 *  well as by what it types, since Shift makes them `{` and `}` on one layout and not on
 *  another. */
export function taskKeyOf(e: KeyboardEvent, mac: boolean): TaskKey | undefined {
  if (!e.shiftKey || e.altKey) return undefined;
  if (mac ? !e.metaKey || e.ctrlKey : !e.ctrlKey || e.metaKey) return undefined;
  if (e.key === "J" || e.key === "j") return "menu";
  if (e.key === "H" || e.key === "h") return "own";
  if (e.code === "BracketRight" || e.key === "]" || e.key === "}") return "next";
  if (e.code === "BracketLeft" || e.key === "[" || e.key === "{") return "previous";
  return undefined;
}

const LETTER: Readonly<Record<TaskKey, string>> = { menu: "J", next: "]", previous: "[", own: "H" };

/** How the rows and the docs spell each key, on this platform. */
export function taskKeySaid(key: TaskKey, mac: boolean): string {
  return mac ? `⌘⇧${LETTER[key]}` : `Ctrl+Shift+${LETTER[key]}`;
}

/** The catalogue's row each key presses. */
export const TASK_KEY_ROW: Readonly<Record<TaskKey, string>> = {
  menu: "tasks.menu",
  next: "tasks.next",
  previous: "tasks.previous",
  own: "tasks.own",
};
