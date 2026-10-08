/**
 * **How a session's tasks are counted, the same on every surface that counts them** (#1487):
 * a tab's chip, the explorer's one line for a workspace's tasks (#1490) and the session's own
 * row (#1491) all call {@link taskBucketOf} and {@link taskCounts}, so they cannot say
 * different numbers of the same tasks.
 *
 * Four counts, and every task is in exactly one:
 *
 * | Count | A task whose state is | Drawn as |
 * |---|---|---|
 * | `working` | working, asking the chat that asked for it, or on a harness that says nothing | ring |
 * | `waiting` | needs you, or idle | pause |
 * | `failed` | failed, ended without a report, or finished in a way the core does not fold | cross |
 * | `done` | done, cancelled, reported, or finished in a way the core folds | tick |
 *
 * **A ring means a program is running or a chat has the next move**, and no ring means
 * nothing is at work: the line the Chats list's own order draws (`chatsList.rankOf`), read
 * from it here and not written a second time. "waiting" is a count's word and not a state's:
 * a row still says `needs you` or `idle`.
 *
 * **A finished row is counted as the core says it folds** (`FinishedTask.folds`), never by the
 * window's reading of its state: a task the person closed reads `cancelled` and does not fold,
 * so it is not one of the done.
 */
import { isLive, rankOf } from "./chatsList";
import type { ShownKind, ShownShape } from "./shownState";
import type { Token } from "./theme/theme";

export type TaskBucket = "working" | "waiting" | "failed" | "done";
export type TaskCounts = Readonly<Record<TaskBucket, number>>;

/** The order the counts are said and drawn in, and kept in as there is less room. */
export const TASK_BUCKETS: readonly TaskBucket[] = ["working", "waiting", "failed", "done"];

/** Each count's shape and colour: its state's own (`shownState`), so a count and a row agree. */
export const TASK_BUCKET_DRAWN: Readonly<Record<TaskBucket, { shape: ShownShape; token: Token }>> =
  {
    working: { shape: "ring", token: "state.running" },
    waiting: { shape: "pause", token: "text.muted" },
    failed: { shape: "cross", token: "state.failed" },
    done: { shape: "tick", token: "text.muted" },
  };

/** Which count an open task in state `kind` is in. */
export function taskBucketOf(kind: ShownKind | undefined): TaskBucket {
  if (rankOf(kind) === 1) return "working";
  if (isLive(kind)) return "waiting";
  return kind === "failed" || kind === "unreported" ? "failed" : "done";
}

/** Which count a finished task is in, by whether the core folds it. */
export function finishedBucketOf(folds: boolean): TaskBucket {
  return folds ? "done" : "failed";
}

/**
 * The counts of a set of tasks: the open ones by their states, the finished ones by whether
 * each folds.
 */
export function taskCounts(
  open: Iterable<ShownKind | undefined>,
  finished: Iterable<boolean> = [],
): TaskCounts {
  const counts = { working: 0, waiting: 0, failed: 0, done: 0 };
  for (const kind of open) counts[taskBucketOf(kind)] += 1;
  for (const folds of finished) counts[finishedBucketOf(folds)] += 1;
  return counts;
}

/** How many tasks were counted. */
export function tasksIn(counts: TaskCounts): number {
  return counts.working + counts.waiting + counts.failed + counts.done;
}

/** The counts in words, nothing for a zero: `2 working, 1 waiting, 3 done`. */
export function taskCountsSaid(counts: TaskCounts): string {
  return TASK_BUCKETS.filter((bucket) => counts[bucket] > 0)
    .map((bucket) => `${counts[bucket]} ${bucket}`)
    .join(", ");
}
