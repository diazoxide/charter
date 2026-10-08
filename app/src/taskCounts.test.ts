import { describe, expect, it } from "vitest";
import { rankOf } from "./chatsList";
import type { ShownKind } from "./shownState";
import {
  TASK_BUCKETS,
  TASK_BUCKET_DRAWN,
  finishedBucketOf,
  taskBucketOf,
  taskCounts,
  taskCountsSaid,
  tasksIn,
} from "./taskCounts";

const KINDS: ShownKind[] = [
  "working",
  "needs-you",
  "asking",
  "done",
  "failed",
  "cancelled",
  "unreported",
  "reported",
  "idle",
  "unheard",
];

describe("how a session's tasks are counted", () => {
  it("puts each state in the one count the ruling names for it", () => {
    expect(Object.fromEntries(KINDS.map((kind) => [kind, taskBucketOf(kind)]))).toEqual({
      working: "working",
      asking: "working",
      unheard: "working",
      "needs-you": "waiting",
      idle: "waiting",
      failed: "failed",
      unreported: "failed",
      done: "done",
      cancelled: "done",
      reported: "done",
    });
  });

  it("calls working exactly what the Chats list ranks as at work", () => {
    for (const kind of KINDS) expect(taskBucketOf(kind) === "working").toBe(rankOf(kind) === 1);
  });

  it("counts a finished task as the core folds it, whatever its state reads", () => {
    expect(finishedBucketOf(true)).toBe("done");
    // Closed by the person: it reads cancelled, the core does not fold it, and it is not done.
    expect(finishedBucketOf(false)).toBe("failed");
    expect(taskCounts([], [true, true, false])).toEqual({
      working: 0,
      waiting: 0,
      failed: 1,
      done: 2,
    });
  });

  it("counts every task once", () => {
    const counts = taskCounts(KINDS, [true, false]);

    expect(counts).toEqual({ working: 3, waiting: 2, failed: 3, done: 4 });
    expect(tasksIn(counts)).toBe(KINDS.length + 2);
  });

  it("says the counts in words, in their order, and nothing for a zero", () => {
    expect(taskCountsSaid({ working: 2, waiting: 1, failed: 0, done: 3 })).toBe(
      "2 working, 1 waiting, 3 done",
    );
    expect(taskCountsSaid({ working: 0, waiting: 0, failed: 0, done: 0 })).toBe("");
  });

  it("draws each count with a shape of its own, the ring for work alone", () => {
    const shapes = TASK_BUCKETS.map((bucket) => TASK_BUCKET_DRAWN[bucket].shape);

    expect(shapes).toEqual(["ring", "pause", "cross", "tick"]);
    expect(new Set(shapes).size).toBe(shapes.length);
  });
});
