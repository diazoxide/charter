/**
 * **Bringing a finished task's row into view in the Chats list** (#1491, V100-15).
 *
 * A task that failed puts the hand on the session that asked for it, and the needs-you item
 * for it goes to the task's row: the row under that session where its end and its report are.
 * This is that going. The row may be folded away under its session, or under a chat above its
 * session, so those are opened first; then the row is scrolled to and given the keyboard, so
 * Enter opens its report.
 *
 * Kept out of `ChatsSection.tsx`, which only calls the hook.
 */
import { useEffect, useRef, type RefObject } from "react";
import type { ChatRow } from "./chatsTree";

/** A finished task to bring into view. */
export type Reveal = {
  /** The session that asked for it, whose finished rows it is among. */
  asker: number;
  /** The task, by the name its finished row has. */
  task: string;
  /** Which asking this is: a new number asks again for the same row. */
  at: number;
};

/** The row of `asker` and of every chat it is drawn under, in `rows`: what must be open for
 *  what is under `asker` to be drawn. */
export function rowsAbove(rows: readonly ChatRow[], asker: number): number[] {
  const at = rows.findIndex((row) => row.session === asker);
  if (at < 0) return [];
  const above = [asker];
  let level = rows[at].level;
  for (let up = at - 1; up >= 0 && level > 1; up -= 1) {
    if (rows[up].level >= level) continue;
    level = rows[up].level;
    above.push(rows[up].session);
  }
  return above;
}

/**
 * The button of the finished row of `task` under the chat called `asker`, inside `within`, or
 * nothing while it is not drawn. Found by what the list says of itself: its group is labelled
 * `Finished tasks of <asker>`, and a row's name is its task's.
 */
export function finishedRowOf(
  within: ParentNode | null,
  asker: string,
  task: string,
): HTMLElement | null {
  if (within === null) return null;
  for (const group of within.querySelectorAll<HTMLElement>('[role="group"][aria-label]')) {
    if (group.getAttribute("aria-label") !== `Finished tasks of ${asker}`) continue;
    for (const row of group.querySelectorAll<HTMLElement>("button.finished-name")) {
      if (row.querySelector(".session")?.textContent === task) return row;
    }
  }
  return null;
}

/**
 * Brings `reveal`'s row into view inside `section`, once for each asking: opens every row it
 * is under (`open(session, false)` is the section's own fold), and when the row is drawn,
 * scrolls to it and puts the keyboard on it. A row that is not there (cleared meanwhile, or
 * not read yet) is waited for across redraws and never invented.
 */
export function useRevealedTask(
  section: RefObject<HTMLElement | null>,
  reveal: Reveal | undefined,
  rows: readonly ChatRow[],
  fold: (session: number, shut: boolean) => void,
): void {
  const shown = useRef<number | undefined>(undefined);
  // After every draw while one is asked for and not yet shown: opening the rows above it
  // draws again, and the row is there on the draw after that.
  useEffect(() => {
    if (reveal === undefined || shown.current === reveal.at) return;
    const above = rowsAbove(rows, reveal.asker);
    for (const session of above) fold(session, false);
    const asker = rows.find((row) => row.session === reveal.asker)?.name;
    if (asker === undefined) return;
    const row = finishedRowOf(section.current, asker, reveal.task);
    if (row === null) return;
    shown.current = reveal.at;
    // Not in every engine a test runs in.
    row.scrollIntoView?.({ block: "nearest" });
    row.focus();
  });
}
