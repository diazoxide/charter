import { useCallback, useState } from "react";

/**
 * Which of the window's four regions are drawn (charter ADR 0038).
 *
 * The centre is not in here: the terminal panes are the product, and a window with no centre
 * is not a state the operator can get into by pressing something.
 */
export type Region = "explorer" | "aside" | "bottom";

export type Shown = Record<Region, boolean>;

/** All three, which is what a window that has never been told otherwise draws. */
export const ALL_SHOWN: Shown = { explorer: true, aside: true, bottom: true };

/** What each region is called, on the button that hides and shows it. */
export const REGION_NAMES: Record<Region, string> = {
  explorer: "Explorer",
  aside: "Attention",
  bottom: "State",
};

/** Where the answer is kept between launches. */
const KEY = "charter.regions.shown";

/**
 * Whether a region is drawn, remembered.
 *
 * **Hiding a region unmounts it rather than sizing it to nothing**, and that is a decision
 * with a cost, so it is written down here. `react-resizable-panels` can collapse a panel to
 * zero and keep it mounted, and doing it that way would keep each region's fetched answers
 * across a hide. It would also give the window two sources of truth for "is the explorer
 * shown" — the library's measured size and whatever the button drew itself from — which
 * disagree in every environment that does not lay out (a test, a window mid-restore), and
 * one of them would be wrong on screen. One source of truth, and a region coming back asks
 * the plane again, which it would have done anyway: nothing in these regions is cached
 * across a workspace focus, because the plane is a directory the operator also edits by hand.
 *
 * **This is not what #125 protects.** A project that is not in front stays mounted and keeps
 * its chats, its splits and its terminals; that is about a project the operator glanced away
 * from. A hidden region is the operator saying to put it away.
 *
 * `localStorage` rather than the plane: this is how one operator likes one window laid out,
 * not something a plane carries to another machine. It is read through a `try` because a
 * webview can refuse storage, and a window must not fail to draw over a layout preference.
 */
export function useRegions(): {
  shown: Shown;
  toggle: (region: Region) => void;
} {
  const [shown, setShown] = useState<Shown>(remembered);

  const toggle = useCallback((region: Region) => {
    setShown((was) => {
      const next = { ...was, [region]: !was[region] };
      remember(next);
      return next;
    });
  }, []);

  return { shown, toggle };
}

/** What was remembered, or all three when nothing readable was. */
export function remembered(): Shown {
  try {
    const held: unknown = JSON.parse(globalThis.localStorage?.getItem(KEY) ?? "null");
    if (held === null || typeof held !== "object") return ALL_SHOWN;
    const said = held as Partial<Record<Region, unknown>>;
    // Each region read on its own, so a stored answer written by an older build — or by
    // hand — brings back the regions it names and leaves the rest drawn. A region charter
    // cannot read the answer for is SHOWN: the failure of a preference must not be the
    // operator losing a region with no way to notice it went.
    return {
      explorer: said.explorer === false ? false : true,
      aside: said.aside === false ? false : true,
      bottom: said.bottom === false ? false : true,
    };
  } catch {
    return ALL_SHOWN;
  }
}

function remember(shown: Shown): void {
  try {
    globalThis.localStorage?.setItem(KEY, JSON.stringify(shown));
  } catch {
    // A webview that will not store it still draws it. The operator loses the preference at
    // the next launch and nothing else.
  }
}
