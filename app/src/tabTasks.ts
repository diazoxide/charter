/**
 * **What a session's tab says of its tasks** (#1487, V100-32, V100-33): the counts its chip
 * wears, the lines its menu lists, and which chat is the next one. Pure: the rows are the
 * tab's own (`tabChats.chatsOfTab`), each chat's state is the one function's (`shownState`),
 * the counts are the one rule's (`taskBuckets.ts`), and nothing here is state, so the chip, the
 * menu and the shortcuts cannot disagree.
 */
import type { ChatStates } from "./chatState";
import { kindsOf as standing, readOfRow } from "./sessionTasks";
import type { ChatRow } from "./chatsTree";
import type { Shown, ShownKind } from "./shownState";
import type { Placed } from "./tabs";
import {
  taskBucketOf,
  taskCounts,
  taskCountsSaid,
  type TaskBucket,
  type TaskCounts,
} from "./taskBuckets";

/** Each row's state, in the rows' order: what a chip and a menu are held on, so a chat that
 *  moves without changing its state redraws neither. */
export function kindsOf(states: ChatStates, rows: readonly ChatRow[]): (ShownKind | undefined)[] {
  // The one reading of what every row says (`sessionTasks.kindsOf`), which the Chats list's
  // rows, order and counts take too: a task waiting on its own tasks is at work on the chip
  // as it is on its row. `rows` is the tree's order, each chat before the chats under it.
  return standing(states, rows.map(readOfRow)).map((stood) => stood.shown?.kind);
}

/**
 * A task of this tab that has ended and still has a line: one of the session's finished rows
 * (`finished.ts`), or the task the tab is still showing (`tabs.shownLive`). It is not in the
 * list of open chats, so it is handed in beside the rows.
 */
export type Ended = {
  key: string;
  /** Its chat's number while the tab still shows it: what marks its line as the one shown. */
  session?: number;
  /** The chat that asked for it. */
  asker: number;
  name: string;
  persona: string | null;
  /** How it ended, as its row says. */
  shown: Shown;
  /** The core's own words for how it ended, where they say more than the state's word. */
  qualifier?: string;
  /** Whether the core folds it into Finished (n) (`FinishedTask.folds`): done and cancelled
   *  do, and nothing else does, whatever its state reads. */
  folds: boolean;
  /** The count it is in: by how the core says it ended (`finishedBucketOf`), or by the state
   *  it ended in where no finished row says. Not `folds`: a task the person closed stands
   *  alone and is one of the done. */
  bucket: TaskBucket;
  /** The workspace it worked in, where that is not its session's. */
  elsewhere: string | null;
  /** Its report, as written: its line opens it. */
  report?: string;
};

/** The counts a tab's chip wears: its tasks, never a pane's own chat (level 1). */
export function countsOf(
  rows: readonly ChatRow[],
  kinds: readonly (ShownKind | undefined)[],
  ended: readonly Ended[],
): TaskCounts {
  return taskCounts(
    kinds.filter((_, at) => rows[at].level > 1),
    ended.map((task) => task.bucket),
  );
}

/** A chip's name: whose tasks, and the counts in words. */
export function chipSaid(session: string, counts: TaskCounts): string {
  // Read aloud in a name: commas, where a row draws its dots.
  const said = taskCountsSaid(counts, { separator: ", " });
  return said === "" ? `Tasks of ${session}` : `Tasks of ${session}: ${said}`;
}

/** One line of a tab's menu. */
export type Line = {
  key: string;
  /** Its chat's number: an open chat's, or an ended task's while the tab still shows it. */
  session?: number;
  name: string;
  persona: string | null;
  /** 1 for a pane's own chat; a task is one more than the chat that asked for it. A line in
   *  the fold is drawn at 2, and says who asked for it in words. */
  level: number;
  /** The chat that asked for it, by name, where that is not the session's own chat: the
   *  words for what the indent draws, and all that says it once a line is in the fold. */
  askedBy: string | null;
  /** The workspace it works in, where that is not its session's (V100-40). */
  elsewhere: string | null;
  /** Whether it is the chat the tab shows now. */
  current: boolean;
  /** Where it is, for a task that has a pane of its own and is still this tab's (#1489): in a
   *  tab of its own, or beside its session. A pick brings that forward. */
  placed?: Placed;
  /** The open chat it is, whose state its line reads live. A press goes to it. */
  row?: ChatRow;
  /** How it ended, for a task that has: its line says this and reads nothing. */
  ended?: Shown;
  qualifier?: string;
  /** An ended task's report: a press opens it under the line. */
  report?: string;
};

/**
 * **A tab's menu**: the session's own chat first, then its tasks under who asked for them,
 * open ones and then ended ones.
 *
 * **The fold.** `finished` is the lines of the one "Finished (n)" fold: every ended task the
 * core folds, and every open task that has reported done or cancelled and whose program has
 * not been ended yet. A failure is never among them (V100-9). A task stays out of the fold
 * while anything under it stays out, since what is under it is read by its place: decided from
 * the bottom up, so a finished task whose own tasks all folded folds with them.
 */
export function menuOf(
  rows: readonly ChatRow[],
  kinds: readonly (ShownKind | undefined)[],
  ended: readonly Ended[],
  /** The chat the tab shows now. */
  current: number | undefined,
): { lines: Line[]; finished: Line[] } {
  const lines: Line[] = [];
  const finished: Line[] = [];
  const endedOf = new Map<number, Ended[]>();
  for (const task of ended) endedOf.set(task.asker, [...(endedOf.get(task.asker) ?? []), task]);

  // The rows under each row, and the row each is under.
  const under = new Map<number, number[]>();
  const above = new Map<number, number>();
  const path: number[] = [];
  rows.forEach((row, at) => {
    while (path.length > 0 && rows[path[path.length - 1]].level >= row.level) path.pop();
    const parent = path[path.length - 1];
    if (parent !== undefined) {
      under.set(parent, [...(under.get(parent) ?? []), at]);
      above.set(at, parent);
    }
    path.push(at);
  });

  // Whether row `at` goes into the fold with everything under it: from the bottom up.
  const folds = new Map<number, boolean>();
  for (let at = rows.length - 1; at >= 0; at -= 1) {
    const row = rows[at];
    folds.set(
      at,
      row.level > 1 &&
        taskBucketOf(kinds[at]) === "done" &&
        (under.get(at) ?? []).every((below) => folds.get(below) === true) &&
        (endedOf.get(row.session) ?? []).every((task) => task.folds),
    );
  }

  /** The workspace of the pane's own chat a row is under. */
  const homeOf = (at: number): string => {
    let top = at;
    for (let up = above.get(top); up !== undefined; up = above.get(top)) top = up;
    return rows[top].workspace;
  };
  const askerOf = (at: number): string | null => {
    const parent = above.get(at);
    return parent === undefined || rows[parent].level === 1 ? null : rows[parent].name;
  };
  const lineOf = (at: number): Line => {
    const row = rows[at];
    return {
      key: `chat:${row.session}`,
      session: row.session,
      name: row.name,
      persona: row.persona,
      level: row.level,
      askedBy: askerOf(at),
      elsewhere: row.workspace === homeOf(at) ? null : row.workspace,
      current: row.session === current,
      ...(row.placed === undefined ? {} : { placed: row.placed }),
      row,
    };
  };
  const endedLine = (task: Ended, asker: ChatRow | undefined): Line => ({
    key: task.key,
    session: task.session,
    name: task.name,
    persona: task.persona,
    level: (asker?.level ?? 1) + 1,
    askedBy: asker === undefined || asker.level === 1 ? null : asker.name,
    elsewhere: task.elsewhere,
    current: task.session !== undefined && task.session === current,
    ended: task.shown,
    qualifier: task.qualifier,
    report: task.report,
  });
  const inTheFold = (line: Line): Line => ({ ...line, level: 2 });

  const walk = (at: number, folded: boolean) => {
    const row = rows[at];
    const into = folded || folds.get(at) === true;
    if (into) finished.push(inTheFold(lineOf(at)));
    else lines.push(lineOf(at));
    for (const below of under.get(at) ?? []) walk(below, into);
    for (const task of endedOf.get(row.session) ?? []) {
      const line = endedLine(task, row);
      if (task.folds) finished.push(inTheFold(line));
      else lines.push(line);
    }
  };
  rows.forEach((_, at) => {
    if (above.get(at) === undefined) walk(at, false);
  });
  // An ended task whose asker is not a chat of this tab any more: still a line, at the end.
  const listed = new Set(rows.map((row) => row.session));
  for (const task of ended) {
    if (listed.has(task.asker)) continue;
    const line = endedLine(task, undefined);
    if (task.folds) finished.push(line);
    else lines.push(line);
  }
  return { lines, finished };
}

/**
 * **The next (`step` 1) or previous (-1) chat in a tab**, in the menu's order and round its
 * ends (V100-36). From a chat that is not listed any more (a task that ended), the next is the
 * first and the previous the last. Nothing in a tab with one chat.
 */
export function neighbour(
  rows: readonly ChatRow[],
  current: number | undefined,
  step: 1 | -1,
): number | undefined {
  if (rows.length === 0) return undefined;
  const at = rows.findIndex((row) => row.session === current);
  if (at < 0) return rows[step === 1 ? 0 : rows.length - 1].session;
  if (rows.length < 2) return undefined;
  return rows[(at + step + rows.length) % rows.length].session;
}

/**
 * How long the pointer rests on a chip before its menu opens by itself (V100-33). A pointer
 * crossing the strip is over a chip for far less, so a pass opens nothing.
 */
export const REST_MS = 350;

/** How long the pointer may be off both the chip and a menu its rest opened before that menu
 *  closes: the time to cross the gap between them, and to come back from a slip off the edge. */
export const GRACE_MS = 300;

/** How far a resting pointer may drift, in pixels. Further than this is moving, and the rest
 *  starts again from where it is. */
export const REST_DRIFT = 4;

/** A chat of the tab that is waiting for the person and is not on screen. */
export type Needing = { session: number; name: string };

/** What a tab's hand says, by the names of the chats that wait: the one that has waited
 *  longest, and how many more there are. */
export function needsSaid(names: readonly string[]): string {
  const [first, ...more] = names;
  return more.length === 0 ? `${first} needs you` : `${first} and ${more.length} more need you`;
}

/** Whether a tab has tasks to count: one that is open, or one that ended and has a line. */
export function hasTasks(rows: readonly ChatRow[], ended: readonly Ended[]): boolean {
  return rows.some((row) => row.level > 1) || ended.length > 0;
}

/** Whether a tab wears a chip: it has a task, or a chat of it is waiting off screen. A
 *  session with neither looks as it always did (V100-32). */
export function wearsChip(
  rows: readonly ChatRow[],
  ended: readonly Ended[],
  needs: readonly Needing[],
): boolean {
  return hasTasks(rows, ended) || needs.length > 0;
}
