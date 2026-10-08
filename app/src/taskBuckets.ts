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
 * | `waiting` | needs you, idle, or a row that says nothing yet | pause |
 * | `failed` | failed, blocked, or ended without a report | cross |
 * | `done` | done, cancelled, reported, or stopped or closed by the person | tick |
 *
 * **A ring means a program is running or a chat has the next move**, and no ring means
 * nothing is at work: the line the Chats list's own order draws (`chatsList.rankOf`), read
 * from it here and not written a second time. "waiting" is a count's word and not a state's:
 * a row still says `needs you` or `idle`.
 *
 * **A finished row is counted by how it ended** (`FinishedTask.how`), the core's own value,
 * and never by whether it folds: a task the person stopped or closed did not fail, so it is
 * one of the done, though its row stands alone and never folds (`FinishedTask.folds` is what
 * says which rows fold, and nothing here reads it).
 */
import type { FinishedTask } from "./bindings";
import { rankOf } from "./chatsList";
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
  if (kind === undefined || kind === "needs-you" || kind === "idle") return "waiting";
  return kind === "failed" || kind === "unreported" ? "failed" : "done";
}

/** Which count a finished task is in, by how the core says it ended. A value this window
 *  does not know (a later core's) is not one of the done: it is counted with the failures,
 *  where it is looked at. */
export function finishedBucketOf(how: FinishedTask["how"]): TaskBucket {
  return how === "done" || how === "cancelled" || how === "stopped_by_person" ? "done" : "failed";
}

/**
 * The counts of a set of tasks: the open ones by their states, and the finished ones by the
 * count each is in (`finishedBucketOf`, or `taskBucketOf` of the state a task ended in where
 * no finished row says how).
 */
export function taskCounts(
  open: Iterable<ShownKind | undefined>,
  finished: Iterable<TaskBucket> = [],
): TaskCounts {
  const counts = { working: 0, waiting: 0, failed: 0, done: 0 };
  for (const kind of open) counts[taskBucketOf(kind)] += 1;
  for (const bucket of finished) counts[bucket] += 1;
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
