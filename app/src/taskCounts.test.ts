import { describe, expect, it } from "vitest";
import type { FinishedTask } from "./bindings";
import { nothingKnown, type ChatStates, type State } from "./chatState";
import type { ListedChat } from "./chatsTree";
import { shownState } from "./shownState";
import { bucketsSaid, NO_TASKS } from "./taskBuckets";
import { atLimitOf, NONE_BELOW, sameBelow, taskCountOf, tasksBelowOf } from "./taskCounts";

/**
 * **Which rows are a session's tasks, and its count of them** (#1491): what its row says,
 * what it is waiting on (`waiting on 2 tasks`), and its limit (`6 of 6 tasks`), all read from
 * the rows the Chats list draws: the open tasks below it and its finished rows.
 */

/** A chat the person started, or a task (or handoff) of `parent`. */
function listed(
  session: number,
  parent: number | null = null,
  more: Partial<ListedChat> = {},
): ListedChat {
  return {
    session,
    name: `chat ${session}`,
    persona: null,
    workspace: "alpha",
    shell: false,
    parent,
    mode: parent === null ? null : "task",
    from: parent === null ? null : `chat ${parent}`,
    tab: parent === null,
    branch: null,
    report: parent === null ? null : "owed",
    outcome: null,
    asking: null,
    harness: "Claude Code",
    ...more,
  };
}

function finished(id: string, asker: number, outcome = "done"): FinishedTask {
  return {
    id,
    asker,
    name: `task ${id}`,
    persona: null,
    how: outcome === "done" || outcome === "cancelled" ? outcome : "failed",
    outcome,
    folds: outcome === "done" || outcome === "cancelled",
    report: "",
    changed: null,
    ended: null,
    place: "alpha",
    branch: null,
    reopens: false,
    not_reopened: null,
  };
}

/** What the chats are doing: each chat's board state, and who is in the queue. */
function states(by: Record<number, State>, needsYou: number[] = []): ChatStates {
  return { ...nothingKnown, bySession: by, needsYou };
}

describe("which rows are a session's tasks", () => {
  it("are the open tasks below it at any depth, and the finished rows under it and them", () => {
    const chats = [listed(1), listed(2, 1), listed(3, 2), listed(4, 1), listed(9)];
    const rows = new Map([
      [1, [finished("a", 1)]],
      [2, [finished("b", 2, "failed")]],
      [9, [finished("c", 9)]],
    ]);

    const below = tasksBelowOf(chats, rows);

    expect(
      below.get(1)?.open.map((task) => `${task.session}${task.direct ? " direct" : ""}`),
    ).toEqual(["2 direct", "3", "4 direct"]);
    expect(below.get(1)?.finished.map((task) => task.id)).toEqual(["a", "b"]);
    // A task with a task of its own is a session to that task.
    expect(below.get(2)?.open.map((task) => task.session)).toEqual([3]);
    expect(below.get(2)?.finished.map((task) => task.id)).toEqual(["b"]);
    // A chat with only finished rows still has a count, and one with nothing has no entry.
    expect(below.get(9)?.open).toEqual([]);
    expect(below.get(9)?.finished.map((task) => task.id)).toEqual(["c"]);
    expect(below.has(3)).toBe(false);
    expect(below.has(4)).toBe(false);
  });

  it("does not count what is below a handoff, which is a session of its own", () => {
    const chats = [listed(1), listed(2, 1, { mode: "handoff", report: null }), listed(3, 2)];

    const below = tasksBelowOf(chats, new Map());

    expect(below.has(1)).toBe(false);
    expect(below.get(2)?.open.map((task) => task.session)).toEqual([3]);
  });

  it("counts each chat once where the lineage loops", () => {
    const chats = [listed(1, 2), listed(2, 1)];

    const below = tasksBelowOf(chats, new Map());

    expect(below.get(1)?.open.map((task) => task.session)).toEqual([2]);
    expect(below.get(2)?.open.map((task) => task.session)).toEqual([1]);
  });

  it("carries the limit the core said for the session", () => {
    const below = tasksBelowOf([listed(1), listed(2, 1)], new Map(), (session) =>
      session === 1 ? 6 : null,
    );
    expect(below.get(1)?.limit).toBe(6);
  });
});

describe("a session's count", () => {
  const chats = [
    listed(1),
    listed(2, 1),
    listed(3, 1),
    listed(4, 1, { report: "sent", outcome: "done" }),
    listed(5, 1, { report: "sent", outcome: "failed" }),
    listed(6, 1, { report: "failed", outcome: "stopped" }),
    listed(7, 1, { report: "failed", outcome: null }),
    listed(8, 1, { asking: "chat 1" }),
  ];
  const rows = new Map([
    [1, [finished("a", 1), finished("b", 1, "cancelled"), finished("c", 1, "blocked")]],
  ]);
  const below = tasksBelowOf(chats, rows).get(1) ?? NONE_BELOW;
  const now = states({
    2: "running",
    3: "waiting",
    4: "waiting",
    5: "waiting",
    6: "done",
    7: "failed",
    8: "waiting",
  });

  it("puts every open task in the bucket its own row's state says, and every finished row in the one the core's fold says", () => {
    // Working: 2 (running) and 8 (asking its asker). Waiting: 3 (idle). Done: 4, 6 (stopped
    // reads cancelled) and the two rows that fold. Failed: 5, 7 and the row that does not.
    expect(taskCountOf(now, below)).toEqual({ working: 2, waiting: 1, done: 4, failed: 3 });
    expect(bucketsSaid(taskCountOf(now, below))).toBe("2 working · 1 waiting · 4 done · 3 failed");
  });

  it("counts a task that needs you as waiting, not as one the session waits on", () => {
    const count = taskCountOf(states({ 2: "waiting", 3: "running", 8: "running" }, [2]), below);
    expect(count.waiting).toBe(1);
    expect(count.working).toBe(2);
  });

  it("is what the session's state says it is waiting on: the working ones only", () => {
    const working = taskCountOf(now, below).working;
    expect(
      shownState({
        board: "waiting",
        needsYou: false,
        task: null,
        harness: null,
        tasksAtWork: working,
      }),
    ).toMatchObject({ word: "waiting on 2 tasks" });
  });

  it("counts a task whose turn has ended while a task of its own works as working", () => {
    const nested = [listed(1), listed(2, 1), listed(3, 2)];
    const under = tasksBelowOf(nested, new Map()).get(1) ?? NONE_BELOW;
    // Task 2 is at rest, and waits on task 3, which is running: both are working.
    expect(taskCountOf(states({ 2: "waiting", 3: "running" }), under)).toMatchObject({
      working: 2,
      waiting: 0,
    });
    // Once task 3 is at rest too, both are waiting.
    expect(taskCountOf(states({ 2: "waiting", 3: "waiting" }), under)).toMatchObject({
      working: 0,
      waiting: 2,
    });
  });

  it("is nothing for a session with no tasks", () => {
    expect(taskCountOf(now, NONE_BELOW)).toBe(NO_TASKS);
  });
});

describe("a session at its limit", () => {
  const six = [listed(1), ...[2, 3, 4, 5, 6, 7].map((session) => listed(session, 1))];
  const limited = (chats: ListedChat[], limit: number | null) =>
    tasksBelowOf(chats, new Map(), () => limit).get(1) ?? NONE_BELOW;

  it("says so when the tasks it asked for itself are as many as it may have running", () => {
    const below = limited(six, 6);
    expect(atLimitOf(states({}), below)).toEqual({ running: 6, limit: 6 });
    expect(bucketsSaid(taskCountOf(states({}), below), atLimitOf(states({}), below))).toBe(
      "6 of 6 tasks",
    );
  });

  it("is not at it below the number, nor where the core said no limit", () => {
    expect(atLimitOf(states({}), limited(six.slice(0, 6), 6))).toBeNull();
    expect(atLimitOf(states({}), limited(six, null))).toBeNull();
  });

  it("counts only its own tasks that have not finished, not the tasks below them", () => {
    // Five of its own at work, and one of those has a task of its own: six working below it,
    // five against its limit.
    const chats = [
      listed(1),
      ...[2, 3, 4, 5, 6].map((session) => listed(session, 1)),
      listed(7, 2),
    ];
    const below = limited(chats, 6);
    expect(taskCountOf(states({}), below).working).toBe(6);
    expect(atLimitOf(states({}), below)).toBeNull();
    // One of its own that waits on the person still counts against its limit.
    const waiting = states({ 2: "waiting" }, [2]);
    expect(atLimitOf(waiting, limited(six, 6))).toEqual({ running: 6, limit: 6 });
    // And one of its own that has reported is not running.
    const reported = [...six.slice(0, 6), listed(7, 1, { report: "sent", outcome: "done" })];
    expect(atLimitOf(states({}), limited(reported, 6))).toBeNull();
  });
});

describe("whether a session's tasks changed", () => {
  it("is the same for the same rows, and different when a task's record or a row changes", () => {
    const chats = [listed(1), listed(2, 1)];
    const one = tasksBelowOf(chats, new Map([[1, [finished("a", 1)]]])).get(1) ?? NONE_BELOW;
    const again = tasksBelowOf(chats, new Map([[1, [finished("a", 1)]]])).get(1) ?? NONE_BELOW;
    expect(sameBelow(one, again)).toBe(true);

    const reported = tasksBelowOf(
      [listed(1), listed(2, 1, { report: "sent", outcome: "done" })],
      new Map([[1, [finished("a", 1)]]]),
    ).get(1);
    expect(sameBelow(one, reported ?? NONE_BELOW)).toBe(false);
    const cleared = tasksBelowOf(chats, new Map()).get(1);
    expect(sameBelow(one, cleared ?? NONE_BELOW)).toBe(false);
  });
});
