import { useState, type FocusEvent, type KeyboardEvent } from "react";
import type { Offer } from "./actions";

/**
 * **Which item of a strip or a list is its one Tab stop** (charter-app#189): the props a
 * `RovingFocusGroup.Root` from `@radix-ui/react-roving-focus` is given, and nothing else.
 *
 * The primitive does the work — the arrows, Home and End, the `tabindex` on every item, and
 * focus landing on the `active` item when the keyboard comes in. What it leaves open is which
 * item is the stop *before anyone has touched the group*: left alone it is none of them, and
 * the group's own element takes the stop instead, which puts a `tabindex="0"` on a `nav` and
 * none on the tab the Authoring Practices say should carry it. So this answers it:
 *
 * - **at rest, the selected item is the stop** — the selected tab, the current explorer row —
 *   or the first one drawn when the selected one is not drawn (a tab the strip collapsed into
 *   its show-more menu, a row inside a closed folder);
 * - **while the keyboard is inside, the item it is on is the stop**, so Tab leaves from there
 *   rather than landing on the selected tab a second time;
 * - **when the keyboard leaves, the stop goes back to the selected item**, which is where
 *   the WAI-ARIA "Tabs" pattern says coming back in lands;
 * - **the group itself is never a stop** (`tabIndex: -1`), because one of its items always is.
 *
 * **How a new strip or list adopts it** (the vaults panel, a vault tab's strip): wrap the
 * container in `RovingFocusGroup.Root asChild orientation=…` with these props spread on it, and
 * each item in `RovingFocusGroup.Item asChild tabStopId={id} active={selected}`. `ids` is every
 * item drawn, in any order; the primitive reads the order from the document.
 *
 * @param selected The item that is selected or current, when there is one.
 * @param ids Every item the group draws right now, by the `tabStopId` it was given.
 */
export function useTabStop(selected: string | undefined, ids: readonly string[]) {
  /** Where the keyboard moved to inside the group, while it is still inside. */
  const [moved, setMoved] = useState<string | null>(null);
  const home = selected !== undefined && ids.includes(selected) ? selected : (ids[0] ?? null);
  const stop = moved !== null && ids.includes(moved) ? moved : home;
  return {
    currentTabStopId: stop,
    onCurrentTabStopIdChange: setMoved,
    onBlur: (event: FocusEvent<HTMLElement>) => {
      const to = event.relatedTarget;
      if (!(to instanceof Node) || !event.currentTarget.contains(to)) setMoved(null);
    },
    tabIndex: -1,
  };
}

/**
 * **Delete on a focused tab closes it** (charter-app#239): the optional key of the WAI-ARIA
 * "Tabs" pattern, called from the `onKeyDown` of each project and chat tab.
 *
 * It is the keyboard's way to the `×`, which is not a Tab stop (fifty closers would be fifty
 * stops); before this, a Mac keyboard reached it only through the palette, having no
 * context-menu key to open the tab's menu with. **It presses the row the `×` presses**, so it
 * asks what the `×` asks: ending a chat asks first (`EndingChat`), a view tab closes without
 * asking because nothing runs in it, and closing a project says what the project's `×` says.
 *
 * - **Delete, and Backspace on a Mac.** The Mac key marked "delete" sends Backspace — Delete is
 *   fn+delete — and it is the key Finder, Mail and every Mac list delete the selected item
 *   with. Elsewhere Backspace on a tab means nothing, and it is left alone.
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
 * Puts the focus on `strip`'s stop once `tab` has left the document, if the focus was left on
 * the page when it did.
 *
 * It watches until the tab goes, however long that is: a chat's tab goes only after the
 * operator answers `EndingChat`, and a Cancel leaves it where it was. A watch left behind by a
 * Cancel acts only if that same tab is later closed with nothing focused, and then it does the
 * same right thing.
 */
function backOnTheStripWhenGone(tab: HTMLElement, strip: HTMLElement) {
  const watch = new MutationObserver(() => {
    if (tab.isConnected) return;
    watch.disconnect();
    const at = document.activeElement;
    if (at !== null && at !== document.body) return;
    strip.querySelector<HTMLElement>('[role="tab"][tabindex="0"]')?.focus();
  });
  watch.observe(strip, { childList: true, subtree: true });
}

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
