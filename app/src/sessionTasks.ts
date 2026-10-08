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
import { shownState, type RowFacts, type Shown } from "./shownState";
import { bucketOfKind, bucketsOf, NO_TASKS, type AtLimit, type TaskBuckets } from "./taskBuckets";

/** One open task below a session, as its own row's state is derived (`RowFacts`). */
export type TaskBelow = {
  session: number;
  /** The chat that asked for it: the session, or a task between them. */
  asker: number | null;
  /** A shell tab shows no state until something reports one. */
  shell: boolean;
  /** Whether the session asked for it itself, and not a task below the session. */
  direct: boolean;
  /** Whether the person asked for it themselves, from its session's tab: the session did not
   *  ask for it and is not waiting on it or on anything below it, though it is counted. */
  byYou: boolean;
} & RowFacts;

/** A session's tasks: the rows its count is a count of. */
export type TasksBelow = {
  /** The open tasks below it, at any depth, in the order they are listed. */
  open: readonly TaskBelow[];
  /** The finished rows under it and under every open task below it. */
  finished: readonly FinishedTask[];
  /** The most tasks it may have running at once, where the core said. */
  limit: number | null;
  /** How many it has running against that limit, as the core counts them for a dispatch from
   *  it (`OpenChat.tasks_running`), where the core said. */
  running: number | null;
};

/** A session with no tasks. */
export const NONE_BELOW: TasksBelow = { open: [], finished: [], limit: null, running: null };

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
  /** The limit in force for a chat, where the caller holds it apart from the chat. Left out,
   *  it is the chat's own (`ListedChat.tasksLimit`). */
  limitOf?: (session: number) => number | null,
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
          byYou: task.byYou === true,
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
    below.set(chat.session, {
      open: tasks,
      finished: rows,
      limit: limitOf === undefined ? (chat.tasksLimit ?? null) : limitOf(chat.session),
      running: chat.tasksRunning ?? null,
    });
  }
  return below;
}

/** What the one pass over a list of chats reads of each: a row of the Chats list is one, and
 *  so is an open task below a session. */
export type Read = {
  session: number;
  /** The chat that asked for it as a task, where one did and it is among the chats read. */
  asker: number | null;
  shell: boolean;
  /** A task the person asked for: not something its asker waits on. */
  byYou?: boolean;
} & RowFacts;

/** What the pass answers of each chat. */
export type Stood = {
  /** The state its row says. */
  shown: Shown | undefined;
  /** How many tasks below it, at any depth, are working, and are ones it waits on: what
   *  `waiting on n tasks` says. */
  atWork: number;
};

/**
 * **The state every chat's row says, read once for a whole list** (#1491): the ONE reading,
 * which a row, the list's order and filter, the count and a tab's chip all take, so a chat
 * reads the same on each.
 *
 * A chat whose turn has ended while tasks below it work says `waiting on n tasks`, and what a
 * chat says changes what the chats above it are waiting on. So the chats are read deepest
 * first: `chats` lists each chat before the tasks it asked for, and each is told how many
 * below it are working.
 *
 * **A task the person asked for is not waited on** by the chat it is under (M4), nor is
 * anything below it: that chat asked for nothing there. It is still counted.
 */
export function kindsOf(states: ChatStates, chats: readonly Read[]): Stood[] {
  const queue = new Set(states.needsYou);
  const working = new Map<number, number>();
  const stood: Stood[] = new Array<Stood>(chats.length);
  for (let at = chats.length - 1; at >= 0; at -= 1) {
    const chat = chats[at];
    const atWork = working.get(chat.session) ?? 0;
    const shown = shownState({
      board: markOf(states, chat.session, chat.shell),
      needsYou: queue.has(chat.session),
      task:
        chat.report === null
          ? null
          : { report: chat.report, outcome: chat.outcome, asking: chat.asking },
      harness: chat.harness,
      tasksAtWork: atWork,
    });
    stood[at] = { shown, atWork };
    if (chat.asker === null || chat.byYou === true) continue;
    const own = bucketOfKind(shown?.kind) === "working" ? 1 : 0;
    working.set(chat.asker, (working.get(chat.asker) ?? 0) + own + atWork);
  }
  return stood;
}

/** A row of the Chats list, as the pass reads it: its asker is the chat it is a task of. */
export function readOfRow(row: ListedChat): Read {
  return {
    session: row.session,
    asker: row.mode === "task" ? row.parent : null,
    shell: row.shell,
    byYou: row.byYou === true,
    report: row.report,
    outcome: row.outcome,
    asking: row.asking,
    harness: row.harness,
  };
}

/**
 * How every row of a list stands, by its chat's number (`kindsOf`): what the list's own
 * order, filter and clock read, so they agree with the word each row draws. `rows` is the
 * tree's order, each chat before the chats under it.
 */
export function standingOfRows(
  states: ChatStates,
  rows: readonly ListedChat[],
): ReadonlyMap<number, Stood> {
  const stood = kindsOf(states, rows.map(readOfRow));
  return new Map(rows.map((row, at) => [row.session, stood[at]]));
}

/** What one reading of a session's tasks comes to. */
type Counted = { count: TaskBuckets; atWork: number };

function counted(states: ChatStates, below: TasksBelow): Counted {
  if (below.open.length === 0 && below.finished.length === 0) return { count: NO_TASKS, atWork: 0 };
  const stood = kindsOf(states, below.open);
  let atWork = 0;
  below.open.forEach((task, at) => {
    // What the session itself waits on: its own tasks that work, and what each waits on.
    if (!task.direct || task.byYou) return;
    atWork += (bucketOfKind(stood[at].shown?.kind) === "working" ? 1 : 0) + stood[at].atWork;
  });
  return {
    count: bucketsOf(
      stood.map((one) => one.shown?.kind),
      below.finished,
    ),
    atWork,
  };
}

/**
 * **A session's count**, given what the chats are doing now: its open tasks each in the
 * bucket of the state its row says, and its finished rows each in the bucket the core's
 * word for how it ended puts it in (`taskBuckets.bucketsOf`).
 */
export function taskCountOf(states: ChatStates, below: TasksBelow): TaskBuckets {
  return counted(states, below).count;
}

/**
 * **How many tasks a session is waiting on**: the working ones below it, at any depth, less
 * a task the person asked for and what is below that. What its row says in `waiting on n
 * tasks` (`shownState`'s `tasksAtWork`). A session whose only open tasks wait on the person
 * waits on none, and the hand on a task rolls up instead.
 */
export function tasksAtWorkOf(states: ChatStates, below: TasksBelow): number {
  return counted(states, below).atWork;
}

/**
 * Whether the session is at its limit, and at what: the count a dispatch from it is decided
 * over, as the core sent it, where that is as many as it may have running. So the row says
 * `6 of 6 tasks` exactly when a seventh would be refused. Nothing below the limit, and
 * nothing where the core said neither.
 */
export function atLimitOf(below: TasksBelow): AtLimit | null {
  if (below.limit === null || below.running === null || below.limit <= 0) return null;
  return below.running >= below.limit ? { running: below.running, limit: below.limit } : null;
}

/** Whether two sessions' tasks are the same rows, so one that did not change keeps the list
 *  its row was drawn from. */
export function sameBelow(one: TasksBelow, other: TasksBelow): boolean {
  return (
    one === other ||
    (one.limit === other.limit &&
      one.running === other.running &&
      one.open.length === other.open.length &&
      one.finished.length === other.finished.length &&
      one.open.every((task, at) => {
        const now = other.open[at];
        return (
          task.session === now.session &&
          task.asker === now.asker &&
          task.shell === now.shell &&
          task.direct === now.direct &&
          task.byYou === now.byYou &&
          task.report === now.report &&
          task.outcome === now.outcome &&
          task.asking === now.asking &&
          task.harness === now.harness
        );
      }) &&
      one.finished.every(
        (task, at) => task.id === other.finished[at].id && task.how === other.finished[at].how,
      ))
  );
}
