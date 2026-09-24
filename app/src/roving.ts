import { useState, type FocusEvent } from "react";

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
