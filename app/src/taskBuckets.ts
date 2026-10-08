/**
 * **A session's tasks in four buckets** (#1491, the dispatcher's ruling for the three surfaces
 * that count them: a session's row, its tab's chip and the explorer's line).
 *
 * Each task is in exactly one bucket, and a bucket that is zero is not said:
 *
 * - **working** (the ring): working, asking the chat that dispatched it, or on a harness
 *   nothing is heard from. A task that is itself waiting on its own tasks is working.
 * - **waiting** (the pause shape, muted): it needs the person, it is idle, or its row says
 *   nothing yet.
 * - **failed** (the cross): failed, blocked, ended without a report, did not start.
 * - **done** (the tick): done, cancelled, reported, and a task the person stopped or closed.
 *   The person ending a task is not the task failing: its row never folds, and it is done.
 *
 * Plain inputs and no state: the state kinds the open tasks' rows say (`shownState`) and the
 * finished rows' `how`, the core's own word for how each ended. Nothing here reads a chat, a store or the core.
 */
import type { ShownKind } from "./shownState";

/** How many of a session's tasks are in each bucket. */
export type TaskBuckets = { working: number; waiting: number; done: number; failed: number };

/** Which bucket. */
export type TaskBucket = keyof TaskBuckets;

/** No tasks at all. */
export const NO_TASKS: TaskBuckets = { working: 0, waiting: 0, done: 0, failed: 0 };

/** The bucket of an open task whose row says `kind`. Working is what the list ranks as at
 *  work (`chatsList.rankOf`'s rank 1). A row that says nothing yet is not at work that
 *  anybody has heard: waiting. */
export function bucketOfKind(kind: ShownKind | undefined): TaskBucket {
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
 *  (`FinishedTask.how`). Every other end is done: `done`, `cancelled`, and
 *  `stopped_by_person`, which is the person's doing and not the task's. */
const CAME_TO_NOTHING: ReadonlySet<string> = new Set([
  "failed",
  "blocked",
  "unreported",
  "did_not_start",
]);

/** The bucket of a finished row, by how the core says it ended. **Not by whether it folds**: a
 *  task the person stopped or closed never folds, and is done all the same. */
export function bucketOfFinished(row: { how: string }): TaskBucket {
  return CAME_TO_NOTHING.has(row.how) ? "failed" : "done";
}

/** **The one count**: the open tasks' state kinds and the finished rows, each in one bucket. */
export function bucketsOf(
  open: Iterable<ShownKind | undefined>,
  finished: Iterable<{ how: string }> = [],
): TaskBuckets {
  const count = { ...NO_TASKS };
  for (const kind of open) count[bucketOfKind(kind)] += 1;
  for (const row of finished) count[bucketOfFinished(row)] += 1;
  return count;
}

/** Whether two counts say the same. */
export function sameBuckets(one: TaskBuckets, other: TaskBuckets): boolean {
  return (
    one === other ||
    (one.working === other.working &&
      one.waiting === other.waiting &&
      one.done === other.done &&
      one.failed === other.failed)
  );
}

/** How many tasks in all. */
export function totalOf(count: TaskBuckets): number {
  return count.working + count.waiting + count.done + count.failed;
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
 * each part only when it is not zero, and nothing for a session with no tasks.
 *
 * **At its limit it says `6 of 6 tasks`** in place of the working and waiting parts, which are
 * the tasks that count against the limit: the next dispatch is refused until one finishes.
 */
export function bucketsSaid(count: TaskBuckets, atLimit?: AtLimit | null): string | undefined {
  const open =
    atLimit != null
      ? [`${atLimit.running} of ${atLimit.limit} tasks`]
      : [
          count.working > 0 ? `${count.working} working` : undefined,
          count.waiting > 0 ? `${count.waiting} waiting` : undefined,
        ];
  // Failed before done: what came to nothing is said before what needs no look.
  const parts = [
    ...open,
    count.failed > 0 ? `${count.failed} failed` : undefined,
    count.done > 0 ? `${count.done} done` : undefined,
  ].filter((part) => part !== undefined);
  return parts.length === 0 ? undefined : parts.join(" · ");
}
