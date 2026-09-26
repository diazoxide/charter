/**
 * The key that opens a shell tab (SI-5): **⌘⇧T on a Mac, Ctrl+Shift+T everywhere else.**
 *
 * **Why these.** Ctrl+Shift+T is "new tab" in GNOME Terminal, Konsole and Windows Terminal —
 * the terminals whose own tabs ARE shells — so an operator's fingers already know it there.
 * On a Mac the same chord takes ⌘, the platform's modifier, as `textSize.ts`'s zoom keys do.
 * (⌘⇧T is a browser's "reopen closed tab"; this window is not a browser and has no such thing.)
 *
 * **It takes nothing from a chat**, measured against what already claims keys and against the
 * pinned `@xterm/xterm` 6.0.0. The palette has `F2` and `⌘K`/`Ctrl-K` (`Palette.opensIt`), the
 * text sizes `⌘`/`Ctrl` with `=`, `-` and `0` (`textSize.sizeKey`), a focused tab `F2` and
 * `Delete` (`tabKeys.ts`), and the native menu `⌘Q`, `⌘,`, the Edit items and, off macOS,
 * `Ctrl+Shift+Q` (`lifecycle.rs`). None is a `T`. xterm.js sends nothing for a `⌘` chord but
 * `⌘A`, and encodes `Ctrl` with a letter only when Shift is NOT held — so plain Ctrl+T still
 * reaches the shell as `^T` (transpose-chars), and Ctrl+Shift+T was never a byte at all. A
 * Mac's `Ctrl` chords stay the terminal's whatever is held with them, and Alt is never this
 * key. `docs/ui-primitives.md` has the rule this follows.
 */
export function opensAShell(e: KeyboardEvent, mac: boolean): boolean {
  if (e.key !== "T" && e.key !== "t") return false;
  if (!e.shiftKey || e.altKey) return false;
  return mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
}

/** How the rows and the docs spell that key, on this platform. */
export function shellKeySaid(mac: boolean): string {
  return mac ? "⌘⇧T" : "Ctrl+Shift+T";
}
