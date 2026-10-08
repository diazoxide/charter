/**
 * **What a session's tab says of its tasks** (#1487, V100-32, V100-33): the counts its chip
 * wears, the lines its menu lists, and which chat is the next one. Pure: the rows are the
 * tab's own (`tabChats.chatsOfTab`), each chat's state is the one function's (`shownState`),
 * and nothing here is state, so the chip, the menu and the shortcuts cannot disagree.
 */
import { markOf, type ChatStates } from "./chatState";
import type { ChatRow } from "./chatsTree";
import { shownState, type Shown, type ShownKind } from "./shownState";

/**
 * The three things a chip counts. Every task is in exactly one, so a tab that has tasks
 * always has a count to wear:
 *
 * - `working`: it has not ended. Working, asking its asker, waiting for the person (the hand
 *   beside the counts says that part), idle, or on a harness that says nothing.
 * - `failed`: failed, or ended without a report. Never folded into the finished count.
 * - `done`: done, cancelled, or reported with no outcome on record.
 */
export type Bucket = "working" | "failed" | "done";
export type Counts = Readonly<Record<Bucket, number>>;

/** The order a chip draws its counts in, and gives them up in from the end when the tab is
 *  too narrow for all of them (`App.css`, `.tab-tasks`). */
export const BUCKETS: readonly Bucket[] = ["working", "failed", "done"];

/** Which count a task in state `kind` is in. */
export function bucketOf(kind: ShownKind | undefined): Bucket {
  if (kind === "failed" || kind === "unreported") return "failed";
  if (kind === "done" || kind === "cancelled" || kind === "reported") return "done";
  return "working";
}

/** What `row`'s chat shows as its state, through the one function. */
export function shownOf(
  states: ChatStates,
  row: Pick<ChatRow, "session" | "shell" | "report" | "outcome" | "asking" | "harness">,
): Shown | undefined {
  return shownState({
    board: markOf(states, row.session, row.shell),
    needsYou: states.needsYou.includes(row.session),
    task:
      row.report === null ? null : { report: row.report, outcome: row.outcome, asking: row.asking },
    harness: row.harness,
  });
}

/** Each row's state, in the rows' order: what a chip and a menu are held on, so a chat that
 *  moves without changing its state redraws neither. */
export function kindsOf(states: ChatStates, rows: readonly ChatRow[]): (ShownKind | undefined)[] {
  return rows.map((row) => shownOf(states, row)?.kind);
}

/**
 * A task of this tab that has ended and still has a line: the task the tab is still showing
 * (`tabs.shownLive`), and, where the window holds them, the session's finished rows. It is
 * not in the list of open chats, so it is handed in beside the rows.
 */
export type Ended = {
  key: string;
  /** Its chat's number while the tab still shows it: what marks its line as the one shown. */
  session?: number;
  /** The chat that asked for it. */
  asker: number;
  name: string;
  persona: string | null;
  /** How it ended, as its row last said. */
  shown: Shown;
};

/** The counts tab's chip wears: its tasks, never a pane's own chat (level 1). */
export function countsOf(
  rows: readonly ChatRow[],
  kinds: readonly (ShownKind | undefined)[],
  ended: readonly Ended[],
): Counts {
  const counts = { working: 0, failed: 0, done: 0 };
  rows.forEach((row, at) => {
    if (row.level > 1) counts[bucketOf(kinds[at])] += 1;
  });
  for (const one of ended) counts[bucketOf(one.shown.kind)] += 1;
  return counts;
}

/** How many tasks a chip counts. */
export function tasksIn(counts: Counts): number {
  return counts.working + counts.failed + counts.done;
}

/** The counts in words, nothing for a zero: `2 working, 3 done`. */
export function countsSaid(counts: Counts): string {
  return BUCKETS.filter((bucket) => counts[bucket] > 0)
    .map((bucket) => `${counts[bucket]} ${bucket}`)
    .join(", ");
}

/** A chip's name: whose tasks, and the counts in words. */
export function chipSaid(session: string, counts: Counts): string {
  const said = countsSaid(counts);
  return said === "" ? `Tasks of ${session}` : `Tasks of ${session}: ${said}`;
}

/** One line of a tab's menu. */
export type Line = {
  key: string;
  /** The chat a press shows. None for an ended task the tab is not showing: nothing to show. */
  session?: number;
  name: string;
  persona: string | null;
  /** 1 for a pane's own chat; a task is one more than the chat that asked for it. */
  level: number;
  /** The workspace it works in, where that is not its session's (V100-40). */
  elsewhere: string | null;
  /** Whether it is the chat the tab shows now. */
  current: boolean;
  /** The open chat it is, whose state its line reads live. */
  row?: ChatRow;
  /** How it ended, for a task that has: its line says this and reads nothing. */
  ended?: Shown;
};

/**
 * **A tab's menu**: the session's own chat first, then its tasks under who asked for them.
 * Done and cancelled tasks are `finished`, the lines of the one "Finished (n)" fold; a failure
 * is never among them (V100-9). A finished task stays out of the fold while a task it asked
 * for is listed under it, since that task's line is read by its place.
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
  let home = rows[0]?.workspace;
  rows.forEach((row, at) => {
    if (row.level === 1) home = row.workspace;
    const line: Line = {
      key: `chat:${row.session}`,
      session: row.session,
      name: row.name,
      persona: row.persona,
      level: row.level,
      elsewhere: row.workspace === home ? null : row.workspace,
      current: row.session === current,
      row,
    };
    const below = (rows[at + 1]?.level ?? 0) > row.level;
    const folds = row.level > 1 && !below && bucketOf(kinds[at]) === "done";
    (folds ? finished : lines).push(folds ? { ...line, level: 2 } : line);
  });
  for (const one of ended) {
    const line: Line = {
      key: one.key,
      session: one.session,
      name: one.name,
      persona: one.persona,
      level: 2,
      elsewhere: null,
      current: one.session !== undefined && one.session === current,
      ended: one.shown,
    };
    (bucketOf(one.shown.kind) === "done" ? finished : lines).push(line);
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
 * **When each chat came into its state, as this window saw it** (V100-19). The core sends a
 * count of moves and no clock (`Moved.moved_at`), so the time is the window's own, taken when
 * it sees a chat's shown state change. **Nothing is guessed**: a chat already in its state
 * when the window first read it has no time, and neither has the first word about a chat
 * nothing had been heard from, which may be about a state that is hours old.
 */
export type SinceClock = {
  /** Reads every chat's state as it now stands, at `now` (ms). */
  read: (kinds: ReadonlyMap<number, ShownKind | undefined>, now: number) => void;
  /** When chat `session` came into its state (ms), or nothing where the window did not see. */
  since: (session: number) => number | null;
  subscribe: (listener: () => void) => () => void;
};

export function sinceClock(): SinceClock {
  const seen = new Map<number, { kind: ShownKind | undefined; at: number | null }>();
  const listeners = new Set<() => void>();
  /** Whether a list has been read: a chat first seen after that has just arrived. */
  let read = false;
  return {
    read: (kinds, now) => {
      let changed = false;
      for (const [session, kind] of kinds) {
        const was = seen.get(session);
        if (was === undefined) {
          seen.set(session, { kind, at: read ? now : null });
          changed ||= read;
        } else if (was.kind !== kind) {
          const at = was.kind === undefined || was.kind === "unheard" ? null : now;
          changed ||= at !== was.at;
          was.kind = kind;
          was.at = at;
        }
      }
      for (const session of [...seen.keys()]) {
        if (kinds.has(session)) continue;
        seen.delete(session);
        changed = true;
      }
      if (kinds.size > 0) read = true;
      if (changed) for (const listener of [...listeners]) listener();
    },
    since: (session) => seen.get(session)?.at ?? null,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
  };
}

/** A time in a state as a line says it: `now`, `40s`, `12m`, `3h`, `2d`. */
export function sinceSaid(seconds: number): string {
  if (seconds < 1) return "now";
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3_600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3_600)}h`;
  return `${Math.floor(seconds / 86_400)}d`;
}

/**
 * How long the pointer rests on a chip before its menu opens by itself (V100-33). A pointer
 * crossing the strip is over a chip for far less, so a pass opens nothing.
 */
export const REST_MS = 350;

/** How long the pointer may be off both the chip and its menu before the menu closes: the
 *  time to cross the gap between them, and to come back from a slip off the edge. */
export const GRACE_MS = 300;

/** How far a resting pointer may drift, in pixels. Further than this is moving, and the rest
 *  starts again from where it is. */
export const REST_DRIFT = 4;

/** A chat of the tab that is waiting for the person and is not on screen. */
export type Needing = { session: number; name: string };

/** What a tab's hand says: the chat that has waited longest, and how many more there are. */
export function needsSaid(needs: readonly Needing[]): string {
  const [first, ...more] = needs;
  return more.length === 0
    ? `${first.name} needs you`
    : `${first.name} and ${more.length} more need you`;
}

/** Whether a tab wears a chip: it has a task, open or ended and still on a line, or a chat of
 *  it is waiting off screen. A session with none looks as it always did (V100-32). */
export function wearsChip(
  rows: readonly ChatRow[],
  ended: readonly Ended[],
  needs: readonly Needing[],
): boolean {
  return rows.some((row) => row.level > 1) || ended.length > 0 || needs.length > 0;
}
