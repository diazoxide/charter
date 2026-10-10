import { useSyncExternalStore } from "react";

/**
 * **Which Chats row's card is up: one at a time, in the whole window** (#1687). Each row holds
 * its own card (a popover with the tooltip's role), so a card the keyboard brought up on one row
 * and one the pointer brings up on another could both be up; a card coming up here takes down
 * whichever was, as an editor's hovers do.
 *
 * **And the skip delay.** While a card is up, or for {@link CARD_SKIP_MS} after the last went
 * down, the pointer resting on another row brings its card up at once rather than after the
 * whole rest: a person reading one row's card and moving to the next is not running down the
 * list. Only the pointer skips: the arrows going through the tree still wait on each row, so a
 * run down it never flashes a card per row.
 */

/** How long after the last card went down the pointer still brings the next up at once. */
export const CARD_SKIP_MS = 300;

/** The row whose card is up, by its chat's number; nothing while none is. */
let up: number | null = null;
/** When the last card went down. */
let downAt = Number.NEGATIVE_INFINITY;
const drawers = new Set<() => void>();

function tell(): void {
  for (const draw of drawers) draw();
}

/** Brings `session`'s card up, and takes down any other. */
export function cardUp(session: number): void {
  if (up === session) return;
  up = session;
  tell();
}

/** Takes `session`'s card down; nothing when another's is up. */
export function cardDown(session: number): void {
  if (up !== session) return;
  up = null;
  downAt = performance.now();
  tell();
}

/** Whether the pointer resting on a row brings its card up at once: a card is up, or one went
 *  down a moment ago. */
export function skipsTheRest(): boolean {
  return up !== null || performance.now() - downAt < CARD_SKIP_MS;
}

/** Whether `session`'s card is up, for its row, which redraws when that changes. */
export function useCardUp(session: number): boolean {
  return useSyncExternalStore(
    (draw) => {
      drawers.add(draw);
      return () => void drawers.delete(draw);
    },
    () => up === session,
  );
}

/** Forgets every card, as a new window would. For tests. */
export function forgetCards(): void {
  up = null;
  downAt = Number.NEGATIVE_INFINITY;
}
