/**
 * The key that opens the project switcher (FR-27): **⌘P on a Mac, Ctrl+Shift+P everywhere
 * else.**
 *
 * **Why these.** P for project, on the platform's own modifier, the way `shellKey.ts` and
 * `SessionPane.opensFind` pick theirs: `⌘` and a letter on a Mac, `Ctrl+Shift` and the letter
 * everywhere else. No native menu item has it (`lifecycle.rs` gives `⌘Q` and `⌘,`), and charter
 * prints nothing, so a Mac's `⌘P` is free.
 *
 * **It takes nothing from a chat**, by the rule `docs/ui-primitives.md` holds every claimed key
 * to, against the pinned `@xterm/xterm` 6.0.0: xterm.js sends nothing for a `⌘` chord but `⌘A`,
 * and encodes `Ctrl` with a letter only when Shift is NOT held — so plain Ctrl+P still reaches
 * the shell as `^P` (previous-history), and Ctrl+Shift+P was never a byte at all. A Mac's `Ctrl`
 * chords stay the terminal's, whatever is held with them, and Alt is never this key.
 */
export function opensTheSwitcher(e: KeyboardEvent, mac: boolean): boolean {
  if (e.key !== "P" && e.key !== "p") return false;
  if (e.altKey) return false;
  return mac ? e.metaKey && !e.ctrlKey && !e.shiftKey : e.ctrlKey && e.shiftKey && !e.metaKey;
}

/** How the rows and the docs spell that key, on this platform. */
export function switcherKeySaid(mac: boolean): string {
  return mac ? "⌘P" : "Ctrl+Shift+P";
}
