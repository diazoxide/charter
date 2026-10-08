/**
 * **A session's tasks in four buckets** (#1491, the dispatcher's ruling for the three surfaces
 * that count them: a session's row, its tab's chip and the explorer's line).
 *
 * Each task is in exactly one bucket, and a bucket that is zero is not said:
 *
 * - **working** (the ring): working, asking the chat that dispatched it, or on a harness
 *   nothing is heard from. A task that is itself waiting on its own tasks is working.
 * - **waiting** (the pause shape, muted): it needs the person, or it is idle.
 * - **failed** (the cross): failed, ended without a report, did not start, and any finished
 *   row the core says does not fold.
 * - **done** (the tick): a finished row the core says folds, and a task that reported.
 *
 * Plain inputs and no state: the state kinds the open tasks' rows say (`shownState`) and the
 * finished rows' `folds`. Nothing here reads a chat, a store or the core.
 */
import type { ShownKind } from "./shownState";

/** How many of a session's tasks are in each bucket. */
export type TaskBuckets = { working: number; waiting: number; done: number; failed: number };

/** Which bucket. */
export type TaskBucket = keyof TaskBuckets;

/** No tasks at all. */
export const NO_TASKS: TaskBuckets = { working: 0, waiting: 0, done: 0, failed: 0 };

/** The bucket of an open task whose row says `kind`. A row that says nothing yet (a shell
 *  nothing has reported for) has not finished and asks for nobody: working. */
export function bucketOfKind(kind: ShownKind | undefined): TaskBucket {
  switch (kind) {
    case "needs-you":
    case "idle":
      return "waiting";
    case "failed":
    case "unreported":
      return "failed";
    // An open task that reported done or was cancelled is a row that will fold.
    case "done":
    case "cancelled":
    case "reported":
      return "done";
    default:
      return "working";
  }
}

/** The bucket of a finished row: done where the core says it folds, failed where it does not
 *  (failed, blocked, ended without a report, closed by the person, did not start). */
export function bucketOfFinished(row: { folds: boolean }): TaskBucket {
  return row.folds ? "done" : "failed";
}

/** **The one count**: the open tasks' state kinds and the finished rows, each in one bucket. */
export function bucketsOf(
  open: Iterable<ShownKind | undefined>,
  finished: Iterable<{ folds: boolean }> = [],
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
 * **What a surface says of a session's tasks**: `2 working · 1 waiting · 3 done · 1 failed`,
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
  const parts = [
    ...open,
    count.done > 0 ? `${count.done} done` : undefined,
    count.failed > 0 ? `${count.failed} failed` : undefined,
  ].filter((part) => part !== undefined);
  return parts.length === 0 ? undefined : parts.join(" · ");
}
