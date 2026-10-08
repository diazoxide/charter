import { describe, expect, it } from "vitest";
import { rankOf } from "./chatsList";
import {
  bucketOfFinished,
  bucketOfKind,
  bucketsOf,
  bucketsSaid,
  NO_TASKS,
  sameBuckets,
  totalOf,
  type TaskBuckets,
} from "./taskBuckets";

/**
 * **A session's tasks in four buckets** (#1491): the one rule its row, its tab's chip and the
 * explorer's line count by. Each task is in exactly one, and a zero is not said.
 */

const count = (more: Partial<TaskBuckets>): TaskBuckets => ({ ...NO_TASKS, ...more });

describe("the bucket of an open task", () => {
  it("is working while it works, asks its asker, or waits on its own tasks", () => {
    for (const kind of ["working", "asking", "unheard", "waiting-on-tasks"] as const)
      expect(bucketOfKind(kind)).toBe("working");
  });

  it("is waiting when it needs the person, is idle, or its row says nothing yet", () => {
    expect(bucketOfKind("needs-you")).toBe("waiting");
    expect(bucketOfKind("idle")).toBe("waiting");
    expect(bucketOfKind(undefined)).toBe("waiting");
  });

  it("is working exactly where the list ranks a chat as at work", () => {
    for (const kind of [
      "working",
      "needs-you",
      "waiting-on-tasks",
      "asking",
      "done",
      "failed",
      "cancelled",
      "unreported",
      "reported",
      "idle",
      "unheard",
      undefined,
    ] as const)
      expect(bucketOfKind(kind) === "working", String(kind)).toBe(rankOf(kind) === 1);
  });

  it("is failed when it failed or ended without a report", () => {
    expect(bucketOfKind("failed")).toBe("failed");
    expect(bucketOfKind("unreported")).toBe("failed");
  });

  it("is done when it reported, however its row words a report that folds", () => {
    for (const kind of ["done", "cancelled", "reported"] as const)
      expect(bucketOfKind(kind)).toBe("done");
  });
});

describe("the bucket of a finished row", () => {
  it("is failed where the task came to nothing, by the core's word for how it ended", () => {
    for (const how of ["failed", "blocked", "unreported", "did_not_start"])
      expect(bucketOfFinished({ how }), how).toBe("failed");
  });

  it("is done for every other end, a task the person stopped or closed among them", () => {
    // Its row never folds, and it is no failure: the person ended it.
    for (const how of ["done", "cancelled", "stopped_by_person"])
      expect(bucketOfFinished({ how }), how).toBe("done");
  });
});

describe("the count", () => {
  it("puts each task in exactly one bucket", () => {
    const counted = bucketsOf(
      ["working", "asking", "needs-you", "idle", "failed", "done"],
      [{ how: "done" }, { how: "stopped_by_person" }, { how: "blocked" }],
    );
    expect(counted).toEqual({ working: 2, waiting: 2, done: 3, failed: 2 });
    expect(totalOf(counted)).toBe(9);
  });

  it("is the same count for the same numbers", () => {
    expect(sameBuckets(count({ working: 1 }), count({ working: 1 }))).toBe(true);
    expect(sameBuckets(count({ working: 1 }), count({ waiting: 1 }))).toBe(false);
  });
});

describe("what a surface says of a session's tasks", () => {
  it("says each bucket in order, working first, and failed before done", () => {
    expect(bucketsSaid(count({ working: 2, waiting: 1, done: 3, failed: 1 }))).toBe(
      "2 working · 1 waiting · 1 failed · 3 done",
    );
  });

  it("does not say a bucket that is zero, and says nothing of a session with no tasks", () => {
    expect(bucketsSaid(count({ working: 2, done: 3 }))).toBe("2 working · 3 done");
    expect(bucketsSaid(count({ done: 5 }))).toBe("5 done");
    expect(bucketsSaid(count({ failed: 1 }))).toBe("1 failed");
    expect(bucketsSaid(NO_TASKS)).toBeUndefined();
  });

  it("says 6 of 6 tasks at its limit, in place of the tasks that count against it", () => {
    expect(bucketsSaid(count({ working: 5, waiting: 1, done: 3 }), { running: 6, limit: 6 })).toBe(
      "6 of 6 tasks · 3 done",
    );
    expect(bucketsSaid(count({ working: 6 }), { running: 6, limit: 6 })).toBe("6 of 6 tasks");
    expect(bucketsSaid(count({ working: 5 }), null)).toBe("5 working");
  });
});
