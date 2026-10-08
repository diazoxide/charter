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
import { useEffect, useRef, useState, type RefObject } from "react";
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
  /** The finished row's own id, its dispatch record's, where the asker of the reveal knows it
   *  (#1491): the row is then found by it, so two tasks of one name are told apart. `task` is
   *  still its name, for what is said of it. */
  id?: string;
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
  /** The row was found, scrolled to and given the keyboard. Told once for each asking. */
  shown?: (reveal: Reveal) => void;
  /** The row could not be shown and is no longer looked for: there is no such row. Told once
   *  for each asking, so whoever asked can say so and do the next best thing. */
  missed?: (reveal: Reveal) => void;
};

/** How many draws a row is looked for across: one that takes a filter off, one that opens
 *  the rows above it, and one that finds it. After that it is not there. */
export const DRAWS_LOOKED = 3;

/** Whether the Chats list is on screen at all: with the left region shut there is no row to
 *  bring into view, and whoever asks for one is told so before asking (#1491). */
export function chatsListDrawn(): boolean {
  return document.querySelector('[data-testid="chats-section"]') !== null;
}

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

/** The button of the finished row whose dispatch record is `id`, inside `within`, or nothing
 *  while it is not drawn (`FinishedTasks` marks each row with it). */
export function finishedRowById(within: ParentNode | null, id: string): HTMLElement | null {
  if (within === null) return null;
  for (const row of within.querySelectorAll<HTMLElement>(".finished-task[data-task-id]")) {
    if (row.getAttribute("data-task-id") === id)
      return row.querySelector<HTMLElement>("button.finished-name");
  }
  return null;
}

/**
 * Brings `reveal`'s row into view inside `section`, once for each asking: takes off a filter
 * that would hide it, opens every row it is under that is shut, and when the row is drawn,
 * scrolls to it, puts the keyboard on it and marks it. A row that is not there (cleared
 * meanwhile, or never there) is never invented.
 *
 * **It is looked for across a few draws and no more** (`DRAWS_LOOKED`, #1491): the draws its
 * own asking causes. After them the asking is over, found or not: a row the person folds
 * again is not opened a second time, and whoever asked is told which it was.
 */
export function useRevealedTask(
  section: RefObject<HTMLElement | null>,
  reveal: Reveal | undefined,
  rows: readonly ChatRow[],
  how: Revealing,
): void {
  const shown = useRef<number | undefined>(undefined);
  /** How many draws this asking has been looked for across. */
  const looked = useRef<{ at: number; draws: number } | undefined>(undefined);
  const [, drawAgain] = useState(0);
  // After each draw while one is asked for and not yet over: taking the filter off and
  // opening the rows above it each draw again, and the row is there on the draw after.
  useEffect(() => {
    if (reveal === undefined || shown.current === reveal.at) return;
    if (looked.current?.at !== reveal.at) looked.current = { at: reveal.at, draws: 0 };
    looked.current.draws += 1;
    /** Not found on this draw: looked for on the next, or given up on. */
    const notYet = () => {
      if ((looked.current?.draws ?? 0) < DRAWS_LOOKED) {
        drawAgain((count) => count + 1);
        return;
      }
      shown.current = reveal.at;
      how.missed?.(reveal);
    };
    const asker = rows.find((row) => row.session === reveal.asker)?.name;
    if (asker === undefined) {
      // A chat's own row that is not listed has closed: nothing takes the keyboard for it
      // later. A finished row's chat may not be read yet, and is looked for once more.
      if (reveal.task === undefined) shown.current = reveal.at;
      else notYet();
      return;
    }
    if (how.hides(reveal)) {
      // Taken off once. A filter that still hides it on the last draw is not fought.
      if (looked.current.draws < DRAWS_LOOKED) how.unfilter(reveal.task ?? asker);
      else notYet();
      return;
    }
    for (const chat of [reveal.asker, ...(reveal.also ?? [])])
      for (const session of rowsAbove(rows, chat)) if (how.shut(session)) how.fold(session, false);
    const row =
      reveal.task === undefined
        ? chatRowOf(section.current, reveal.asker)
        : reveal.id !== undefined
          ? finishedRowById(section.current, reveal.id)
          : finishedRowOf(section.current, asker, reveal.task);
    if (row === null) {
      notYet();
      return;
    }
    shown.current = reveal.at;
    how.shown?.(reveal);
    // Not in every engine a test runs in.
    row.scrollIntoView?.({ block: "nearest" });
    row.focus();
    marked(row);
  });
}
