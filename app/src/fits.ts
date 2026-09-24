/**
 * How many of a strip's tabs it draws, and which ones collapse into its show-more menu.
 *
 * **The strip does not scroll any more** — the operator's call on 2026-09-22, against what
 * ADR 0039 left open ("whether the row still scrolls"), and the amendment to that
 * record is where the argument lives. What does not fit is not drawn, and the show-more
 * button beside the strip is how it is reached.
 *
 * ## Why this is arithmetic and not a measurement of fifty tabs
 *
 * `offscreen.ts` — what this replaces — asked the browser which tabs were wholly inside a
 * scroller, with an `IntersectionObserver` over every tab. That is the right question to ask
 * about a strip that scrolls and the wrong one to ask about a strip that collapses: an
 * intersection is a fact about what has *already been laid out*, so using it to decide what
 * to lay out is a loop — hide a tab, its box is gone, it is not intersecting, it stays
 * hidden forever even when the space comes back.
 *
 * So the tabs are sized the way **every browser sizes its own tabs**: each one takes an equal
 * share of the strip, never narrower than [`Least`] and never wider than its strip's
 * maximum. That one rule makes "how many fit" exact arithmetic — `n` tabs fit in `width`
 * exactly when `width / n >= least` — and the only thing that has to be measured is one
 * number per strip, the strip's own width. Fifty tabs are not measured at all.
 *
 * **The sizing is the browsers'; the overflow is not, and the difference is worth being
 * accurate about.** Every browser shares the row out equally and shrinks to a floor — that
 * part an operator already knows, and it is what makes the arithmetic exact. What they do
 * *past* the floor differs and none of them does what charter does: Chrome and Safari keep
 * going and then scroll, Firefox scrolls with arrows. The show-more menu is ADR 0039's own
 * answer, argued there against exactly those.
 *
 * ## What it costs, said where it bites
 *
 * **A strip of two tabs draws two wide tabs**, because equal shares of the strip is what
 * equal shares means. That is the browser model and it is the Zed screenshot the operator
 * sent; it is not the natural-width strip charter drew before, and the difference is most
 * visible with one or two chats open.
 *
 * **A name too long for the floor is truncated** rather than widening its tab. The whole name
 * is in the tab's `title` and in the palette, which is the surface for reading rather than
 * aiming (ADR 0039).
 *
 * ## The measurement, and the window nobody is drawing
 *
 * The one number is read with `clientWidth` and kept up to date with a `ResizeObserver`, and
 * **neither needs the window to be painting**. That is not a detail: macOS gives a WKWebView
 * no rendering at all while its window is covered or the display is asleep (charter-app
 * M0.6), and the intersection measurement this replaces could not be answered at all on such
 * a runner — `panes.e2e.ts` recorded Linux seeing 3 of 51 tabs and macOS seeing 0 of 49 on
 * the same commit. `clientWidth` is a synchronous layout read; a layout is not a paint.
 *
 * **A width of zero is not a measurement, it is the absence of one**, and everything is drawn
 * then. That is what an environment with no layout gets — jsdom gives every element a
 * zero-sized box (#149) — and it is the right degradation: a strip that drew nothing because
 * nobody had told it how wide it was would be a window with no tabs in it.
 */
import { useCallback, useLayoutEffect, useRef, useState } from "react";

/**
 * The narrowest a tab of a given strip may be drawn, in pixels at {@link LEAST_TUNED_AT} —
 * {@link leastAt} is what a strip fits by at the window text size in force.
 *
 * **These are the numbers the stylesheet fits by, and it is given them from here** — each
 * strip sets `--least` from its own value and `App.css` reads it for `min-width`. Two copies
 * of a number that has to agree is exactly how a strip comes to draw eight tabs while its
 * arithmetic says seven fit, and the disagreement is invisible until somebody has fifty.
 *
 * Three values and not one, because the three strips are three depths (the operator's
 * "PROJECT is holder of workspaces, workspaces are holder of sessions"): a project is the
 * widest thing on the window and a chat is the narrowest.
 */
export const LEAST = {
  /** A project: the full-width segments across the top. */
  project: 132,
  /** A workspace: inside a project, and narrower than one. */
  workspace: 104,
  /** A chat: the innermost, and the one there are fifty of. */
  chat: 88,
} as const;

export type Least = (typeof LEAST)[keyof typeof LEAST];

/** The window text size {@link LEAST} was measured at: the root was 13px until #283. */
export const LEAST_TUNED_AT = 13;

/**
 * A floor at the window text size in force (charter-app#283).
 *
 * {@link LEAST} is in pixels at the default size, and a tab's name is in `rem`: at 20px text a
 * 104px workspace tab holds half the letters it did, and the strip would draw as many tabs as
 * before with each one ellipsised to nothing. So the floor grows and shrinks with the text, and
 * the strip draws fewer, wider tabs — which is what a bigger text size is asking for. The
 * stylesheet is handed the same scaled number, for {@link LEAST}'s reason.
 */
export function leastAt(least: number, windowText: number): number {
  return Math.round((least * windowText) / LEAST_TUNED_AT);
}

/**
 * How many tabs a strip `width` wide can draw, none of them narrower than `least`.
 *
 * **Never zero while the strip has a tab.** A window narrower than one whole tab is a
 * degenerate width, and one squeezed tab is a strip; no tabs at all is a bug that looks like
 * an empty project.
 */
export function capacity(width: number, least: number): number {
  if (width <= 0) return Infinity;
  return Math.max(1, Math.floor(width / least));
}

/**
 * Which tabs a strip draws and which collapse into its menu, in the strip's own order.
 *
 * **The order is never touched** (ADR 0039). This takes a prefix of it and hands back the
 * rest; nothing sorts, and the menu does its own sorting on what it is given.
 *
 * **The selected tab is always drawn**, which is the one exception and is not a re-ordering:
 * it is what the scroller used to do with `scrollIntoView`, and a strip whose selected tab is
 * behind a menu is a strip that cannot say where you are. It takes the place of the last tab
 * that fits, and it is drawn where the order puts it rather than at the end — a tab that
 * jumped to the right-hand edge when it was picked would be the moving target the record
 * refuses.
 */
export function fitting<T>(
  order: readonly T[],
  selected: T | undefined,
  width: number,
  least: number,
): { shown: T[]; hidden: T[] } {
  const room = capacity(width, least);
  if (order.length <= room) return { shown: [...order], hidden: [] };
  const drawn = new Set(order.slice(0, room));
  if (selected !== undefined && order.includes(selected) && !drawn.has(selected)) {
    drawn.delete(order[room - 1]);
    drawn.add(selected);
  }
  return {
    shown: order.filter((one) => drawn.has(one)),
    hidden: order.filter((one) => !drawn.has(one)),
  };
}

/**
 * Watches how wide a strip is, and how much of it its own controls have taken.
 *
 * Two elements rather than one, because the strips are not built alike and neither shape is
 * wrong. The chat strip's `+` and show-more are siblings of the tablist, so the tablist's own
 * width is already what is left for tabs and `controls` is never put on anything. The project
 * strip's buttons are *inside* its tablist — a `role="tab"` has to be owned by the tablist it
 * belongs to — so what is left for tabs is the strip less its controls.
 *
 * **Both are callback refs**, so the watch starts again when the element itself changes rather
 * than only when its contents do. `PlaneView` mounts and unmounts these strips as projects
 * come forward.
 *
 * `holds` is how many things the strip is drawing. It is not used in the arithmetic — it is
 * when to measure again, for the reason at the bottom of this function.
 */
export function useRoom(holds: number): { strip: Ref; controls: Ref; width: number } {
  const [width, setWidth] = useState(0);
  const strip = useRef<Watched>({ element: null });
  const controls = useRef<Watched>({ element: null });

  /** The room there is right now, from whichever of the two elements are there. */
  const measure = useCallback(() => {
    const whole = strip.current.element?.clientWidth ?? 0;
    const taken = controls.current.element?.offsetWidth ?? 0;
    setWidth(Math.max(0, whole - taken));
  }, []);

  // **One observer per element, taken down by the ref that put it up.** React calls a
  // callback ref with `null` when the element goes, and again on a StrictMode remount, so
  // this is the whole lifecycle: nothing is left watching an element that has gone, and
  // nothing watches one element twice.
  const stripRef = useCallback(
    (element: HTMLElement | null) => watch(strip, element, measure),
    [measure],
  );
  const controlsRef = useCallback(
    (element: HTMLElement | null) => watch(controls, element, measure),
    [measure],
  );

  // **And again whenever the strip holds a different number of things, which is what makes
  // this answerable on a window nobody is drawing.** A `ResizeObserver` is answered in the
  // engine's rendering step, and macOS gives a WKWebView no rendering while its window is
  // covered or the display is asleep (charter-app M0.6) — the measurement this replaced could
  // not be answered at all on such a runner, and `panes.e2e.ts` recorded it: Linux saw 3 of 51
  // tabs and macOS 0 of 49, same commit. A layout effect reading `clientWidth` is a
  // synchronous layout, and a layout is not a paint. It costs one forced layout on the renders
  // where the count changed — fifty of them across the fifty-tab loop, on one element.
  useLayoutEffect(measure, [holds, measure]);

  return { strip: stripRef, controls: controlsRef, width };
}

/** An element being watched, and the observer watching it. */
type Watched = { element: HTMLElement | null; watching?: ResizeObserver };

/** Points `slot` at `element`, and moves the observer with it. */
function watch(slot: { current: Watched }, element: HTMLElement | null, measure: () => void): void {
  slot.current.watching?.disconnect();
  slot.current = { element };
  if (element && typeof ResizeObserver !== "undefined") {
    const watching = new ResizeObserver(measure);
    watching.observe(element);
    slot.current.watching = watching;
  }
  // **Measured on attach, not only when the observer speaks.** A `ResizeObserver` is answered
  // in the rendering step; this read is synchronous, so a strip is the right width in the
  // frame it appears in rather than one frame later.
  measure();
}

/** What `useRoom` hands back to put on an element. */
export type Ref = (element: HTMLElement | null) => void;
