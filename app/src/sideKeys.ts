import type { RegionId, ViewId } from "./regions";
import { onAMac } from "./tabKeys";

/**
 * **The keys of the sides** (#1673, #1678, B-10): VS Code's, so an editor's fingers carry over.
 *
 * | | On a Mac | Everywhere else |
 * | --- | --- | --- |
 * | Put the navigation region away, or bring it back | `⌘B` | `Ctrl+B` |
 * | Put the attention region away, or bring it back | `⌥⌘B` | `Ctrl+Alt+B` |
 * | Show Explorer | `⌘⇧E` | `Ctrl+Shift+E` |
 * | Show Chats | `⌘⇧C` | `Ctrl+Shift+C` |
 *
 * **What a chat keeps** (`docs/ui-primitives.md`, the palette's rule, charter-app#106). xterm.js
 * 6.0.0 sends nothing for a `⌘` chord but `⌘A`, so on a Mac none of these was a byte. Off a
 * Mac, `Ctrl+B` is one (readline's back-char, tmux's prefix) and `Ctrl+Shift+C` is a Linux
 * terminal's copy, so while a chat has the keyboard both are left to it, as Search's key is
 * left to the find bar; `Ctrl+Shift+E` is no byte (Ctrl with a letter is encoded only without
 * Shift) and is the window's everywhere. A key is matched by what it types.
 *
 * **⌥⌘B is the right side's** (#1678), VS Code's key for its secondary side bar, and it names
 * the region and not the edge, as VS Code's ⌘B follows its primary side bar wherever that is
 * put. On a Mac, Option makes B type `∫`, so a key that types no letter is read by its place, as
 * the palette's ⌥⌘C is (`Palette.fileKeyOf`); a key that types another letter is never this one.
 * Off a Mac, Ctrl with Alt is AltGr on Windows, so only a key that still types `b` is read, and a
 * layout whose AltGr+B types a character keeps it; and since a terminal sends Ctrl+Alt+B (an
 * escape and Ctrl+B), a chat with the keyboard keeps it too.
 *
 * ⌘⇧C is Chats' because it is free: VS Code gives it to an external terminal, which this window
 * has no use for, and ⌘⇧H, ⌘⇧J and ⌘⇧T are taken (`taskKeys.ts`, `shellKey.ts`).
 */
export type SideKey = ({ toggle: RegionId } | { show: ViewId }) & {
  /** A chord a terminal has a use for: left to a chat while one has the keyboard. */
  chatKeeps?: true;
};

export function sideKeyOf(e: KeyboardEvent, mac: boolean): SideKey | undefined {
  const command = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  if (!command) return undefined;
  const letter = e.key.length === 1 ? e.key.toLowerCase() : "";
  if (e.altKey) {
    if (e.shiftKey) return undefined;
    if (letter === "b") return mac ? { toggle: "aside" } : { toggle: "aside", chatKeeps: true };
    // On a Mac, Option+B types `∫`: no letter at all, so the key's place says which it is.
    if (mac && !/^\p{L}$/u.test(e.key) && e.code === "KeyB") return { toggle: "aside" };
    return undefined;
  }
  if (!e.shiftKey) {
    if (letter !== "b") return undefined;
    return mac ? { toggle: "navigation" } : { toggle: "navigation", chatKeeps: true };
  }
  if (letter === "e") return { show: "explorer" };
  if (letter === "c") return mac ? { show: "chats" } : { show: "chats", chatKeeps: true };
  return undefined;
}

/** How the tooltips, the palette's rows and the docs spell the keys, on this platform. */
export function sideKeysSaid(
  mac: boolean,
): Record<"navigation" | "aside", string> & Partial<Record<ViewId, string>> {
  return mac
    ? { navigation: "⌘B", aside: "⌥⌘B", chats: "⌘⇧C", explorer: "⌘⇧E" }
    : {
        navigation: "Ctrl+B",
        aside: "Ctrl+Alt+B",
        chats: "Ctrl+Shift+C",
        explorer: "Ctrl+Shift+E",
      };
}

/** The keys on this platform. */
export const SIDE_KEYS_SAID = sideKeysSaid(onAMac());
