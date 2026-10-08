import { describe, expect, it } from "vitest";
import { rankOf } from "./chatsList";
import type { ShownKind } from "./shownState";
import {
  finishedBucketOf,
  NO_TASKS,
  sameBuckets,
  TASK_BUCKETS,
  TASK_BUCKET_DRAWN,
  taskBucketOf,
  taskCounts,
  taskCountsSaid,
  tasksIn,
  type TaskCounts,
} from "./taskBuckets";

/**
 * **A session's tasks in four counts** (#1487, #1490, #1491): the one rule its row, its tab's
 * chip and the explorer's line count by. Each task is in exactly one, and a zero is not said.
 */

const count = (more: Partial<TaskCounts>): TaskCounts => ({ ...NO_TASKS, ...more });

const KINDS: (ShownKind | undefined)[] = [
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
];

describe("the count of an open task", () => {
  it("puts each state in the one count the ruling names for it", () => {
    expect(Object.fromEntries(KINDS.map((kind) => [String(kind), taskBucketOf(kind)]))).toEqual({
      working: "working",
      asking: "working",
      unheard: "working",
      // A task that is itself waiting on its own tasks is at work.
      "waiting-on-tasks": "working",
      "needs-you": "waiting",
      idle: "waiting",
      // A row that says nothing yet.
      undefined: "waiting",
      failed: "failed",
      unreported: "failed",
      done: "done",
      cancelled: "done",
      reported: "done",
    });
  });

  it("is working exactly where the list ranks a chat as at work", () => {
    for (const kind of KINDS)
      expect(taskBucketOf(kind) === "working", String(kind)).toBe(rankOf(kind) === 1);
  });
});

describe("the count of a finished row", () => {
  it("is failed where the task came to nothing, by the core's word for how it ended", () => {
    for (const how of ["failed", "blocked", "unreported", "did_not_start"])
      expect(finishedBucketOf(how), how).toBe("failed");
  });

  it("is done for every other end, a task the person stopped or closed among them", () => {
    // Its row never folds, and it is no failure: the person ended it.
    for (const how of ["done", "cancelled", "stopped_by_person"])
      expect(finishedBucketOf(how), how).toBe("done");
  });
});

describe("the count", () => {
  it("counts every task once", () => {
    const counted = taskCounts(
      ["working", "asking", "needs-you", "idle", "failed", "done"],
      ["done", "stopped_by_person", "blocked"].map(finishedBucketOf),
    );

    expect(counted).toEqual({ working: 2, waiting: 2, failed: 2, done: 3 });
    expect(tasksIn(counted)).toBe(9);
    expect(tasksIn(taskCounts(KINDS))).toBe(KINDS.length);
  });

  it("is the same count for the same numbers", () => {
    expect(sameBuckets(count({ working: 1 }), count({ working: 1 }))).toBe(true);
    expect(sameBuckets(count({ working: 1 }), count({ waiting: 1 }))).toBe(false);
  });

  it("draws each count with a shape of its own, the ring for work alone", () => {
    const shapes = TASK_BUCKETS.map((bucket) => TASK_BUCKET_DRAWN[bucket].shape);

    expect(shapes).toEqual(["ring", "pause", "cross", "tick"]);
    expect(new Set(shapes).size).toBe(shapes.length);
  });
});

describe("what a surface says of a session's tasks", () => {
  it("says each count in order, working first, and failed before done", () => {
    expect(taskCountsSaid(count({ working: 2, waiting: 1, done: 3, failed: 1 }))).toBe(
      "2 working · 1 waiting · 1 failed · 3 done",
    );
    expect(TASK_BUCKETS).toEqual(["working", "waiting", "failed", "done"]);
  });

  it("does not say a count that is zero, and says nothing of a session with no tasks", () => {
    expect(taskCountsSaid(count({ working: 2, done: 3 }))).toBe("2 working · 3 done");
    expect(taskCountsSaid(count({ done: 5 }))).toBe("5 done");
    expect(taskCountsSaid(count({ failed: 1 }))).toBe("1 failed");
    expect(taskCountsSaid(NO_TASKS)).toBe("");
  });

  it("says the same counts with the separator a surface asks for", () => {
    expect(taskCountsSaid(count({ working: 2, waiting: 1, done: 3 }), { separator: ", " })).toBe(
      "2 working, 1 waiting, 3 done",
    );
  });

  it("says 6 of 6 tasks at its limit, in place of the tasks that count against it", () => {
    const atLimit = { running: 6, limit: 6 };
    expect(taskCountsSaid(count({ working: 5, waiting: 1, done: 3 }), { atLimit })).toBe(
      "6 of 6 tasks · 3 done",
    );
    expect(taskCountsSaid(count({ working: 6 }), { atLimit })).toBe("6 of 6 tasks");
    expect(taskCountsSaid(count({ working: 5 }), { atLimit: null })).toBe("5 working");
  });
});
