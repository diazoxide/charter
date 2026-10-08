/**
 * **Bringing a row into view in the Chats list** (#1491, V100-15; #1490, V100-14): a finished
 * task's row, or a chat's own.
 *
 * A task that failed puts the hand on the session that asked for it, and the needs-you item
 * for it goes to the task's row: the row under that session where its end and its report are.
 * This is that going. The row may be folded away under its session, or under a chat above its
 * session, so those are opened first; then the row is scrolled to and given the keyboard, so
 * Enter opens its report.
 *
 * **The explorer's lines ask for a chat's own row** (#1490): the one line for a session's
 * tasks asks for the session, and the line for the tasks working at a place asks for each of
 * them. Those leave `task` out. The row is opened so what is under it is drawn, a filter that
 * would hide what was asked for is taken off and said to be, and the row is marked for a
 * moment, since a pointer's press draws no focus ring.
 *
 * Kept out of `ChatsSection.tsx`, which only calls the hook.
 */
import { useEffect, useRef, type RefObject } from "react";
import type { ChatRow } from "./chatsTree";

/** A row to bring into view: a finished task's, or a chat's own. */
export type Reveal = {
  /** The session that asked for the task, whose finished rows it is among. With no `task`,
   *  the chat whose own row is asked for. */
  asker: number;
  /** The task, by the name its finished row has. Left out, the row is `asker`'s own (#1490):
   *  the two fields below are that case's, and the only ones #1490 added. */
  task?: string;
  /** More chats whose own rows are asked for with `asker`'s (#1490): each is opened, with the
   *  rows above it, and the keyboard goes on `asker`'s. */
  also?: readonly number[];
  /** Which asking this is: a new number asks again for the same row. */
  at: number;
};

/** What the hook asks of the section it reveals in: its folds and its filter. */
export type Revealing = {
  /** `open(session, false)` is the section's own fold, set as the person sets one. */
  fold: (session: number, shut: boolean) => void;
  /** Whether a row is shut now. Only a shut row is opened: a row open by itself is left to
   *  fold by itself when its tasks are over (V100-48). */
  shut: (session: number) => boolean;
  /** Whether the filter, as it stands, would hide what `reveal` asks for. */
  hides: (reveal: Reveal) => boolean;
  /** Takes the filter off and says so, naming the chat it was taken off for. */
  unfilter: (name: string) => void;
};

/** How long a revealed row stays marked, in milliseconds. */
export const REVEALED_MS = 1500;

/** The row of chat `session` inside `within`, or nothing while it is not drawn. */
export function chatRowOf(within: ParentNode | null, session: number): HTMLElement | null {
  return within?.querySelector<HTMLElement>(`[role="treeitem"][data-session="${session}"]`) ?? null;
}

/**
 * Marks `row` as the one just revealed, for {@link REVEALED_MS}: the stylesheet draws the band
 * a hovered row has and settles it in once. An attribute the hook owns, on an element the list
 * draws: nothing the list draws reads it.
 */
function marked(row: HTMLElement): void {
  row.setAttribute("data-revealed", "");
  window.setTimeout(() => row.removeAttribute("data-revealed"), REVEALED_MS);
}

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
 * Brings `reveal`'s row into view inside `section`, once for each asking: takes off a filter
 * that would hide it, opens every row it is under that is shut, and when the row is drawn,
 * scrolls to it, puts the keyboard on it and marks it. A row that is not there (cleared
 * meanwhile, or not read yet) is waited for across redraws and never invented; a chat that
 * is no longer listed is not waited for.
 */
export function useRevealedTask(
  section: RefObject<HTMLElement | null>,
  reveal: Reveal | undefined,
  rows: readonly ChatRow[],
  how: Revealing,
): void {
  const shown = useRef<number | undefined>(undefined);
  // After every draw while one is asked for and not yet shown: taking the filter off and
  // opening the rows above it each draw again, and the row is there on the draw after.
  useEffect(() => {
    if (reveal === undefined || shown.current === reveal.at) return;
    const asker = rows.find((row) => row.session === reveal.asker)?.name;
    if (asker === undefined) {
      // A finished row's chat may not be read yet. A chat's own row that is not listed has
      // closed: nothing takes the keyboard for it later.
      if (reveal.task === undefined) shown.current = reveal.at;
      return;
    }
    if (how.hides(reveal)) {
      how.unfilter(reveal.task ?? asker);
      return;
    }
    for (const chat of [reveal.asker, ...(reveal.also ?? [])])
      for (const session of rowsAbove(rows, chat)) if (how.shut(session)) how.fold(session, false);
    const row =
      reveal.task === undefined
        ? chatRowOf(section.current, reveal.asker)
        : finishedRowOf(section.current, asker, reveal.task);
    if (row === null) return;
    shown.current = reveal.at;
    // Not in every engine a test runs in.
    row.scrollIntoView?.({ block: "nearest" });
    row.focus();
    marked(row);
  });
}
