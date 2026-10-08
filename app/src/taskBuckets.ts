/**
 * **How a session's tasks are counted, the same on every surface that counts them** (#1487,
 * #1490, #1491): a tab's chip, the session's own row and the explorer's one line all call
 * {@link taskBucketOf}, {@link finishedBucketOf} and {@link taskCounts}, so they cannot say
 * different numbers of the same tasks.
 *
 * Four counts, and every task is in exactly one. A count that is zero is not said.
 *
 * | Count | A task whose state is | Drawn as |
 * |---|---|---|
 * | `working` | working, asking the chat that asked for it, on a harness that says nothing, or itself waiting on its own tasks | ring |
 * | `waiting` | needs you, idle, or a row that says nothing yet | pause |
 * | `failed` | failed, blocked, ended without a report, or did not start | cross |
 * | `done` | done, cancelled, reported, or stopped or closed by the person | tick |
 *
 * **A ring means a program is running or a chat has the next move**, and no ring means
 * nothing is at work: the line the Chats list's own order draws (`chatsList.rankOf`'s rank 1),
 * which a test holds this to. "waiting" is a count's word and not a state's: a row still says
 * `needs you` or `idle`.
 *
 * **A finished row is counted by how it ended** (`FinishedTask.how`), the core's own value,
 * and never by whether it folds: the person ending a task is not the task failing, so it is
 * one of the done, though its row stands alone and never folds (`FinishedTask.folds` is what
 * says which rows fold, and nothing here reads it).
 *
 * Plain inputs and no state: nothing here reads a chat, a store or the core.
 */
import type { ShownKind, ShownShape } from "./shownState";
import type { Token } from "./theme/theme";

export type TaskBucket = "working" | "waiting" | "failed" | "done";
/** How many of a session's tasks are in each count. */
export type TaskCounts = Readonly<Record<TaskBucket, number>>;

/** The order the counts are said and drawn in, and kept in as there is less room: what came
 *  to nothing is said before what needs no look. */
export const TASK_BUCKETS: readonly TaskBucket[] = ["working", "waiting", "failed", "done"];

/** Each count's shape and colour: its state's own (`shownState`), so a count and a row agree. */
export const TASK_BUCKET_DRAWN: Readonly<Record<TaskBucket, { shape: ShownShape; token: Token }>> =
  {
    working: { shape: "ring", token: "state.running" },
    waiting: { shape: "pause", token: "text.muted" },
    failed: { shape: "cross", token: "state.failed" },
    done: { shape: "tick", token: "text.muted" },
  };

/** No tasks at all. */
export const NO_TASKS: TaskCounts = { working: 0, waiting: 0, failed: 0, done: 0 };

/** Which count an open task whose row says `kind` is in. A row that says nothing yet is not
 *  at work that anybody has heard: waiting. */
export function taskBucketOf(kind: ShownKind | undefined): TaskBucket {
  switch (kind) {
    case "working":
    case "asking":
    case "unheard":
    case "waiting-on-tasks":
      return "working";
    case "failed":
    case "unreported":
      return "failed";
    // An open task that reported done or was cancelled is a row that will fold.
    case "done":
    case "cancelled":
    case "reported":
      return "done";
    default:
      return "waiting";
  }
}

/** How a finished row ended that is the task coming to nothing, in the core's words
 *  (`FinishedTask.how`; a task that did not start is sent as `failed`). Every other end is
 *  done: `done`, `cancelled`, and `stopped_by_person`, which is the person's doing and not
 *  the task's. */
const CAME_TO_NOTHING: ReadonlySet<string> = new Set([
  "failed",
  "blocked",
  "unreported",
  "did_not_start",
]);

/** Which count a finished task is in, by how the core says it ended. **Not by whether it
 *  folds**: a task the person stopped or closed never folds, and is done all the same. */
export function finishedBucketOf(how: string): TaskBucket {
  return CAME_TO_NOTHING.has(how) ? "failed" : "done";
}

/**
 * **The one count**: the open tasks by the states their rows say, and the finished ones by
 * the count each is in (`finishedBucketOf` of a finished row's `how`, or `taskBucketOf` of
 * the state a task ended in where no finished row says how yet).
 */
export function taskCounts(
  open: Iterable<ShownKind | undefined>,
  finished: Iterable<TaskBucket> = [],
): TaskCounts {
  const counts = { ...NO_TASKS };
  for (const kind of open) counts[taskBucketOf(kind)] += 1;
  for (const bucket of finished) counts[bucket] += 1;
  return counts;
}

/** Whether two counts say the same. */
export function sameBuckets(one: TaskCounts, other: TaskCounts): boolean {
  return one === other || TASK_BUCKETS.every((bucket) => one[bucket] === other[bucket]);
}

/** How many tasks were counted. */
export function tasksIn(counts: TaskCounts): number {
  return counts.working + counts.waiting + counts.failed + counts.done;
}

/** A session at the most tasks it may have running at once (V100-26). */
export type AtLimit = {
  /** The tasks it asked for itself that have not finished. */
  running: number;
  /** The limit in force for it. */
  limit: number;
};

/**
 * **What a surface says of a session's tasks**: `2 working · 1 waiting · 1 failed · 3 done`,
 * each part only when it is not zero, in the counts' order, and nothing (the empty string) for
 * a session with no tasks. A surface that reads it aloud in a name passes its own separator.
 *
 * **At its limit it says `6 of 6 tasks`** in place of the working and waiting parts, which are
 * the tasks that count against the limit: the next dispatch is refused until one finishes.
 */
export function taskCountsSaid(
  counts: TaskCounts,
  { atLimit, separator = " · " }: { atLimit?: AtLimit | null; separator?: string } = {},
): string {
  const said = (bucket: TaskBucket) => (counts[bucket] > 0 ? [`${counts[bucket]} ${bucket}`] : []);
  const open =
    atLimit != null
      ? [`${atLimit.running} of ${atLimit.limit} tasks`]
      : [...said("working"), ...said("waiting")];
  return [...open, ...said("failed"), ...said("done")].join(separator);
}
