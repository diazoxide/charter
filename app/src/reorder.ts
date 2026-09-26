import { arrayMove } from "@dnd-kit/sortable";

/**
 * **What dragging a tab does to its strip** (SI-6): the order, and whether the drop pinned or
 * unpinned it. Pure, so every rule below is a test in `reorder.test.ts` rather than a thing
 * only a pointer can check.
 *
 * Every strip draws its pinned tabs first (ADR 0039), so a strip is two groups side by side,
 * and a drop is read against them:
 *
 * - **Inside one group, the tab moves to where it was dropped.** That is the only way a
 *   strip's order changes, and it is the operator's hand doing it: ADR 0039's rule that a strip
 *   never reorders itself is about the strip, not the person arranging it.
 * - **Across the boundary, it pins or unpins.** A tab dropped among the pinned ones is pinned
 *   where it landed, and one dropped among the others is unpinned where it landed. The pin is
 *   the same pin the tab's menu sets; a drag is a second way to set it, never a second kind.
 * - **A fixed tab does not move, and nothing takes its place.** It cannot be picked up, a drop
 *   on it does nothing, and the tabs around it are rearranged around it. Nothing is fixed yet:
 *   it is how "Outside every workspace" becomes the workspace strip's first tab without a
 *   second drag rule.
 *
 * `dnd-kit` says which tab was dragged and which it was dropped on; this says what that means.
 */

/** What a drop did to a strip. */
export interface Drop<T> {
  /** The strip in its new order, pinned first. */
  order: T[];
  /** The dragged tab's pin, when the drop carried it across the boundary; absent when it
   *  stayed in its own group. */
  pinned?: boolean;
}

/**
 * The strip after `moved` was dropped on `onto`, or nothing when the drop changed nothing — a
 * tab dropped on itself, a tab the strip does not draw, or a fixed tab on either end.
 */
export function dropped<T>(
  strip: readonly T[],
  moved: T,
  onto: T,
  isPinned: (one: T) => boolean,
  isFixed: (one: T) => boolean = () => false,
): Drop<T> | undefined {
  if (moved === onto || isFixed(moved) || isFixed(onto)) return undefined;
  // The fixed tabs are taken out, the drop is made on what is left, and they go back where
  // they were: so a tab dragged past one never shifts it.
  const loose = strip.filter((one) => !isFixed(one));
  const from = loose.indexOf(moved);
  const to = loose.indexOf(onto);
  if (from < 0 || to < 0) return undefined;
  const order = arrayMove(loose, from, to);
  strip.forEach((one, at) => {
    if (isFixed(one)) order.splice(at, 0, one);
  });
  const crossed = isPinned(moved) !== isPinned(onto);
  return crossed ? { order, pinned: isPinned(onto) } : { order };
}

/**
 * `whole` with the items of `part` put back in the places they held, in `part`'s order.
 *
 * A strip draws some of a longer list — one workspace's chats out of every tab the project has,
 * the tabs that fit out of the ones that do not — and a drag rearranges only what it drew.
 * Everything else keeps its place.
 */
export function reslotted<T>(whole: readonly T[], part: readonly T[]): T[] {
  const taking = new Set(part);
  let next = 0;
  return whole.map((one) => (taking.has(one) ? part[next++] : one));
}

/**
 * `whole`, pinned first, after `moved` was dropped on `onto` on a strip that draws `drawn` of
 * it — or nothing when the drop changed nothing.
 *
 * The drop is made on what was drawn, put back into the whole, and the whole is sorted into
 * its two groups again with the dragged tab's new pin: so a tab pinned by a drop is among the
 * pinned ones wherever the hidden ones were.
 */
export function afterDrop<T>(drop: {
  whole: readonly T[];
  drawn: readonly T[];
  moved: T;
  onto: T;
  isPinned: (one: T) => boolean;
  isFixed?: (one: T) => boolean;
}): Drop<T> | undefined {
  const { whole, drawn, moved, onto, isPinned, isFixed } = drop;
  const made = dropped(drawn, moved, onto, isPinned, isFixed);
  if (made === undefined) return undefined;
  const pinnedNow = (one: T) =>
    one === moved && made.pinned !== undefined ? made.pinned : isPinned(one);
  const all = reslotted(whole, made.order);
  const order = [...all.filter(pinnedNow), ...all.filter((one) => !pinnedNow(one))];
  return made.pinned === undefined ? { order } : { order, pinned: made.pinned };
}
