/**
 * The key that opens a Search tab (FM-8): **⌘⇧F on a Mac, Ctrl+Shift+F everywhere else** — except
 * while a chat has the keyboard off a Mac, where Ctrl+Shift+F is the chat's find bar
 * (`SessionPane.opensFind`) and stays it.
 *
 * **Why these.** ⌘⇧F and Ctrl+Shift+F are "find in files" in every editor an operator already
 * uses. On a Mac ⌘F is the chat's find bar and ⌘⇧F is free; off a Mac the find bar took
 * Ctrl+Shift+F first, so it keeps it inside a chat, and everywhere else in the window the same
 * chord searches the files.
 *
 * **It takes nothing from a chat**, by the rule `docs/ui-primitives.md` holds every claimed key
 * to, against the pinned `@xterm/xterm` 6.0.0: xterm.js sends nothing for a `⌘` chord but `⌘A`,
 * and encodes `Ctrl` with a letter only when Shift is NOT held, so Ctrl+Shift+F was never a byte.
 * A Mac's `Ctrl` chords stay the terminal's, and Alt is never this key.
 */
export function opensSearch(e: KeyboardEvent, mac: boolean): boolean {
  if (e.key !== "F" && e.key !== "f") return false;
  if (!e.shiftKey || e.altKey) return false;
  return mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
}

/** How the rows and the docs spell that key, on this platform. */
export function searchKeySaid(mac: boolean): string {
  return mac ? "⌘⇧F" : "Ctrl+Shift+F";
}
