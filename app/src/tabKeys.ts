import type { KeyboardEvent } from "react";
import type { Offer } from "./actions";

/**
 * **Delete on a focused tab closes it** (charter-app#239): the optional key of the WAI-ARIA
 * "Tabs" pattern, called from the `onKeyDown` of each project and chat tab.
 *
 * It is the keyboard's way to the `×`, which is not a Tab stop (fifty closers would be fifty
 * stops); before this, a Mac keyboard reached it only through the palette, having no
 * context-menu key to open the tab's menu with. **It presses the row the `×` presses**, so it
 * asks what the `×` asks: ending a chat asks first (`EndingChat`), a view tab closes without
 * asking because nothing runs in it, and closing a project with chats open asks first
 * (`ClosingProject`) — the operator's ruling, because on a Mac this key is the ordinary delete.
 *
 * - **Delete, and Backspace on a Mac.** The Mac key marked "delete" sends Backspace — Delete is
 *   fn+delete — and it is the key Mail and Notes delete the selected item with (Finder asks
 *   for ⌘ as well, because what it deletes is a file). Elsewhere Backspace on a tab means
 *   nothing, and it is left alone.
 * - **Never with a modifier**, which leaves every chord to whoever else wants it.
 * - **Only on the tab itself.** The handler is the tab's and it acts only when the key was
 *   pressed ON the tab, so a key in a terminal or a text field — which is never inside a tab —
 *   cannot reach it.
 *
 * **The keyboard stays on the strip.** The tab it was on goes, and an element that goes takes
 * the focus with it to the page, where the next arrow or Delete does nothing. So once the tab
 * is gone — at once for a view, after the question for a chat — and only if the focus went
 * nowhere else meanwhile, the strip's stop takes it: by then the tab in front.
 *
 * Not on a workspace tab: that strip has no `×`, and the one close its menu has deletes the
 * workspace on disk.
 *
 * @param offer The row the tab's `×` runs — `tab.close:<id>` or `project.close:<plane>`.
 */
export function closeOnDelete(
  event: KeyboardEvent<HTMLElement>,
  offer: Offer | undefined,
  onPress: (offer: Offer) => void,
) {
  if (offer === undefined || event.target !== event.currentTarget) return;
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
  if (event.key !== "Delete" && !(event.key === "Backspace" && onAMac())) return;
  event.preventDefault();
  const tab = event.currentTarget;
  const strip = tab.closest<HTMLElement>('[role="tablist"]');
  onPress(offer);
  if (strip) backOnTheStripWhenGone(tab, strip);
}

/**
 * **F2 on a focused chat tab opens its name for editing** (charter-app#254): the platform's
 * rename key for a focused item, called from the same `onKeyDown` as {@link closeOnDelete}.
 *
 * It presses the row the tab's menu and the palette list (`tab.rename:<id>`), so it does what
 * they do. The palette, which claims `F2` from the whole window, stands back on a tab marked
 * `RENAMES_ON_F2` (`Palette.theTabRenamesOnIt`); this is the other half of that. Never with a
 * modifier, and only on the tab itself, for `closeOnDelete`'s reasons.
 *
 * @param offer The tab's `tab.rename:<id>` row. A view's tab has none, and F2 there is the
 *   palette's as everywhere else.
 */
export function renameOnF2(
  event: KeyboardEvent<HTMLElement>,
  offer: Offer | undefined,
  onPress: (offer: Offer) => void,
) {
  if (offer === undefined || event.target !== event.currentTarget) return;
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
  if (event.key !== "F2") return;
  event.preventDefault();
  onPress(offer);
}

/**
 * Puts the focus on `strip`'s stop once `tab` has left the document, if the focus was left on
 * the page when it did.
 *
 * It watches until the tab goes, however long that is: a chat's tab goes only after the
 * operator answers `EndingChat`, and a Cancel leaves it where it was. **One watch per strip**:
 * a second Delete replaces the first, so Delete-then-Cancel pressed again and again leaves one
 * watch behind and not a pile of them, and one left behind acts only if its tab is later
 * closed with nothing focused, where it does the same right thing.
 *
 * The look at the focus waits a turn after the tab goes, because the dialog that asked hands
 * the focus back as it closes, and that can land a moment after the tab it hands it to is gone.
 */
function backOnTheStripWhenGone(tab: HTMLElement, strip: HTMLElement) {
  watching.get(strip)?.disconnect();
  const watch = new MutationObserver(() => {
    if (tab.isConnected) return;
    watch.disconnect();
    watching.delete(strip);
    setTimeout(() => {
      const at = document.activeElement;
      if (at !== null && at !== document.body) return;
      strip.querySelector<HTMLElement>('[role="tab"][tabindex="0"]')?.focus();
    });
  });
  watch.observe(strip, { childList: true, subtree: true });
  watching.set(strip, watch);
}

/** The watch each strip has running, if any. */
const watching = new WeakMap<HTMLElement, MutationObserver>();

/**
 * Whether the keyboard in front of the operator is a Mac's.
 *
 * From the page and not asked of the binary, unlike the title bar's room (`bindings.ts`,
 * `titleBarRoom`): that one decides a layout and has to be exact, and this decides one key,
 * where the platform string WebKit reports on the machine it runs on is the answer.
 */
function onAMac(): boolean {
  return navigator.platform.startsWith("Mac");
}
