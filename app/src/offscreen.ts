/**
 * Which of a strip's tabs the operator cannot see all of, right now.
 *
 * This is the one new fact the show-more menu needs (ADR 0039). The strip scrolls, so "what
 * does not fit" is not a property of the tabs — it is a property of the tabs, the strip's
 * width and where it happens to be scrolled to, and all three change without React being
 * told. So it is measured rather than derived.
 *
 * **`IntersectionObserver`, which is the browser's own answer to this question.** It is the
 * standard tool (`AGENTS.md`'s second priority), and the alternative — reading
 * `getBoundingClientRect` for fifty tabs on every scroll event — is the custom tooling that
 * rule exists to prevent, and it forces a layout per read. The observer computes intersection
 * in the engine's own rendering step, fires only when a tab crosses the threshold, and picks
 * up a scroll, a resize of the window, a resize of the strip and a tab appearing or going,
 * with nothing here listening for any of them.
 *
 * **A tab you can see only half of is one the menu offers.** The menu's job is the shortest
 * path to a tab the strip is not showing, and half a tab is a tab that cannot be read and can
 * barely be aimed at. The cost is stated where it bites: a window narrower than one tab puts
 * every tab in the menu, which is a degenerate width and still leaves everything reachable.
 *
 * **The threshold is [`WHOLLY`] and not `1`, and that is measured rather than cautious.** A
 * threshold of exactly 1 asks for an intersection ratio of exactly 1.0, and a ratio is
 * computed from rectangles the engine lays out in fractions of a pixel: a tab that is wholly
 * on screen routinely measures 0.9999 and is then reported as not intersecting. The scenario
 * run on macOS measured every one of forty-nine visible tabs as hidden for exactly that
 * reason, while the same build on Linux measured three of fifty-one as visible — same code,
 * same threshold, different rounding.
 *
 * **Nothing is ever removed from the DOM by this.** The tabs it names are scrolled out of
 * view, not unmounted: they keep their place in the strip, their `role="tab"`, their close
 * button and their tab stop. That is deliberate and it is the lesson of charter-app#130 —
 * the defect there was a wanted control living inside the thing that hides controls, and a
 * menu that collapsed the strip's tail would be the same hazard in different clothes. The
 * menu is an affordance that says there are more; it is not the only way to reach them.
 */
import { useEffect, useState } from "react";

/** The attribute a strip marks each tab with, so this can find them without knowing the DOM
 *  around them. Its value is the tab's id. */
export const TAB_ATTRIBUTE = "data-tab";

/**
 * How much of a tab has to be inside the strip for it to count as shown.
 *
 * Not `1`. See this module's own docstring: an intersection ratio comes off rectangles laid
 * out in fractions of a pixel, and a whole tab measures 0.9999 often enough that a threshold
 * of 1 reported every visible tab as hidden on one of the two platforms the scenario tests
 * run on. The slack is a hundredth of a tab, which is never the difference between a tab an
 * operator can read and one they cannot.
 */
const WHOLLY = 0.99;

/** One empty answer, reused, so a strip with nothing hidden does not re-render on every
 *  measurement that finds nothing hidden. */
const NOTHING: readonly number[] = [];

export type Offscreen = {
  /** Put on the element that scrolls. A callback ref, so the measurement starts again when
   *  the element itself changes rather than only when its contents do. */
  strip: (element: HTMLElement | null) => void;
  /** The tabs not wholly on screen, in the strip's own order. */
  offscreen: readonly number[];
};

/**
 * Watches `shown` inside whatever element `strip` is put on.
 *
 * `shown` must be a stable array — a `useMemo` result — because a new array on every render
 * would take every observer down and put it back up again on every render. `PlaneView`'s
 * already is.
 */
export function useOffscreen(shown: readonly number[]): Offscreen {
  const [strip, setStrip] = useState<HTMLElement | null>(null);
  const [offscreen, setOffscreen] = useState<readonly number[]>(NOTHING);

  useEffect(() => {
    // No element yet, or no observer — jsdom has none, and a window without one simply has
    // no show-more button. Every tab is still on the strip and still reachable, which is
    // what makes this a degradation rather than a failure.
    if (!strip || typeof IntersectionObserver === "undefined") return;

    /** The tabs found hidden so far. Kept across the observer's own callbacks, because a
     *  callback carries only the tabs whose visibility CHANGED. */
    const hidden = new Set<number>();
    const watching = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const id = Number(entry.target.getAttribute(TAB_ATTRIBUTE));
          if (!Number.isInteger(id)) continue;
          if (entry.isIntersecting) hidden.delete(id);
          else hidden.add(id);
        }
        // In the strip's order, never the observer's: entries arrive in whatever order the
        // engine noticed them, and the menu sorts what it is given by activity, so an
        // arbitrary order here would be an arbitrary tie-break there.
        const found = shown.filter((id) => hidden.has(id));
        setOffscreen((was) => (same(was, found) ? was : found.length === 0 ? NOTHING : found));
      },
      { root: strip, threshold: WHOLLY },
    );
    for (const tab of strip.querySelectorAll(`[${TAB_ATTRIBUTE}]`)) watching.observe(tab);
    return () => {
      watching.disconnect();
    };
  }, [shown, strip]);

  return { strip: setStrip, offscreen };
}

/** Whether two answers are the same, so an unchanged measurement is not a re-render. */
function same(one: readonly number[], other: readonly number[]): boolean {
  return one.length === other.length && one.every((id, at) => id === other[at]);
}
