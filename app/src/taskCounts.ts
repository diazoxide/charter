/**
 * **How a session's tasks stand, counted** (#1491, V100-3, V100-16, V100-26).
 *
 * A session's row says `2 working · 1 waiting · 3 done`, and `· 1 failed` when any did; a session whose
 * turn has ended while tasks below it still work says `waiting on 2 tasks`; and at its limit
 * the row says `6 of 6 tasks`. All three are counts over the same rows the Chats list draws:
 * the open tasks below the session, each in the state its own row says (`shownState`), and the
 * finished rows under it (`finished.ts`). Nothing here is state and nothing here reads the
 * core: every answer is derived from plain values, so a session's row, its tab's chip and a
 * folded row's summary call the same functions and cannot disagree.
 *
 * - `tasksBelowOf` says which rows are a session's: the open tasks below it, at any depth, and
 *   the finished rows under it and under them.
 * - `taskCountOf` counts them, given what the chats are doing now, into the four buckets of
 *   `taskBuckets.ts` (the one bucket function every surface shares).
 * - `taskBuckets.bucketsSaid` is the sentence a row draws, and `shownState`'s `tasksAtWork` is
 *   the `working` bucket alone: a session whose only open tasks wait on the person is not
 *   waiting on tasks, and the hand on a task rolls up instead.
 */
import type { FinishedTask } from "./bindings";
import { markOf, type ChatStates } from "./chatState";
import type { ListedChat } from "./chatsTree";
import { shownState, type RowFacts, type ShownKind } from "./shownState";
import { bucketOfKind, bucketsOf, NO_TASKS, type AtLimit, type TaskBuckets } from "./taskBuckets";

/** One open task below a session, as its own row's state is derived (`RowFacts`). */
export type TaskBelow = {
  session: number;
  /** The chat that asked for it: the session, or a task between them. */
  asker: number;
  /** A shell tab shows no state until something reports one. */
  shell: boolean;
  /** Whether the session asked for it itself, and not a task below the session: what the
   *  session's own limit counts. */
  direct: boolean;
} & RowFacts;

/** A session's tasks: the rows its count is a count of. */
export type TasksBelow = {
  /** The open tasks below it, at any depth, in the order they are listed. */
  open: readonly TaskBelow[];
  /** The finished rows under it and under every open task below it. */
  finished: readonly FinishedTask[];
  /** The most tasks it may have running at once, where the core said. */
  limit: number | null;
};

/** A session with no tasks. */
export const NONE_BELOW: TasksBelow = { open: [], finished: [], limit: null };

/**
 * **Each session's tasks**, by its number: for every listed chat that has any, the open tasks
 * below it and the finished rows under it and them.
 *
 * **Down task links only**, as the core counts what a session waits on: a handoff moved the
 * work to a session of its own (V100-69), so what is below a handoff's chat is that chat's.
 * Every chat is counted once under each chat above it, and a lineage that loops is cut.
 */
export function tasksBelowOf(
  chats: readonly ListedChat[],
  finished: ReadonlyMap<number, readonly FinishedTask[]>,
  /** The limit in force for a chat, where the core said one. */
  limitOf: (session: number) => number | null = () => null,
): ReadonlyMap<number, TasksBelow> {
  const open = new Set(chats.map((chat) => chat.session));
  const started = new Map<number, ListedChat[]>();
  for (const chat of chats) {
    if (chat.mode !== "task" || chat.parent === null || chat.parent === chat.session) continue;
    if (!open.has(chat.parent)) continue;
    started.set(chat.parent, [...(started.get(chat.parent) ?? []), chat]);
  }
  const below = new Map<number, TasksBelow>();
  for (const chat of chats) {
    const tasks: TaskBelow[] = [];
    const rows: FinishedTask[] = [...(finished.get(chat.session) ?? [])];
    const seen = new Set([chat.session]);
    const walk = (asker: number, direct: boolean) => {
      for (const task of started.get(asker) ?? []) {
        if (seen.has(task.session)) continue;
        seen.add(task.session);
        tasks.push({
          session: task.session,
          asker,
          shell: task.shell,
          direct,
          report: task.report,
          outcome: task.outcome,
          asking: task.asking,
          harness: task.harness,
        });
        rows.push(...(finished.get(task.session) ?? []));
        walk(task.session, false);
      }
    };
    walk(chat.session, true);
    if (tasks.length === 0 && rows.length === 0) continue;
    below.set(chat.session, { open: tasks, finished: rows, limit: limitOf(chat.session) });
  }
  return below;
}

/**
 * The state each open task's row says, in the order they are listed, as its own row derives
 * it: a task whose turn has ended while tasks of its own still work is waiting on them, so
 * the tasks are read deepest first and each is told how many below it are working.
 */
function kindsOf(states: ChatStates, open: readonly TaskBelow[]): (ShownKind | undefined)[] {
  const working = new Map<number, number>();
  const kinds: (ShownKind | undefined)[] = new Array<ShownKind | undefined>(open.length);
  // A task is listed before the tasks it asked for, so the last is the deepest.
  for (let at = open.length - 1; at >= 0; at -= 1) {
    const task = open[at];
    const below = working.get(task.session) ?? 0;
    const kind = shownState({
      board: markOf(states, task.session, task.shell),
      needsYou: states.needsYou.includes(task.session),
      task:
        task.report === null
          ? null
          : { report: task.report, outcome: task.outcome, asking: task.asking },
      harness: task.harness,
      tasksAtWork: below,
    })?.kind;
    kinds[at] = kind;
    const own = bucketOfKind(kind) === "working" ? 1 : 0;
    working.set(task.asker, (working.get(task.asker) ?? 0) + own + below);
  }
  return kinds;
}

/**
 * **A session's count**, given what the chats are doing now: its open tasks each in the
 * bucket of the state its row says, and its finished rows each in the bucket the core's
 * `folds` puts it in (`taskBuckets.bucketsOf`).
 */
export function taskCountOf(states: ChatStates, below: TasksBelow): TaskBuckets {
  if (below.open.length === 0 && below.finished.length === 0) return NO_TASKS;
  return bucketsOf(kindsOf(states, below.open), below.finished);
}

/**
 * Whether the session is at its limit, and at what: the tasks it asked for itself that have
 * not finished (working or waiting), where they are as many as it may have running. Nothing
 * below the limit, and nothing where the core said no limit.
 */
export function atLimitOf(states: ChatStates, below: TasksBelow): AtLimit | null {
  if (below.limit === null || below.limit <= 0) return null;
  const kinds = kindsOf(states, below.open);
  const running = below.open.filter((task, at) => {
    const bucket = bucketOfKind(kinds[at]);
    return task.direct && (bucket === "working" || bucket === "waiting");
  }).length;
  return running >= below.limit ? { running, limit: below.limit } : null;
}

/** Whether two sessions' tasks are the same rows, so one that did not change keeps the list
 *  its row was drawn from. */
export function sameBelow(one: TasksBelow, other: TasksBelow): boolean {
  return (
    one === other ||
    (one.limit === other.limit &&
      one.open.length === other.open.length &&
      one.finished.length === other.finished.length &&
      one.open.every((task, at) => {
        const now = other.open[at];
        return (
          task.session === now.session &&
          task.asker === now.asker &&
          task.shell === now.shell &&
          task.direct === now.direct &&
          task.report === now.report &&
          task.outcome === now.outcome &&
          task.asking === now.asking &&
          task.harness === now.harness
        );
      }) &&
      one.finished.every(
        (task, at) => task.id === other.finished[at].id && task.folds === other.finished[at].folds,
      ))
  );
}
