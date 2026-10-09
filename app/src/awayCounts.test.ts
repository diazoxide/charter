import { describe, expect, it } from "vitest";
import type { AwayRefusal, FinishedTask } from "./bindings";
import { nothingKnown, type ChatStates } from "./chatState";
import {
  awayPartsSaid,
  awaySaid,
  awaySummaryOf,
  cameToNeedOf,
  countedAway,
  NOTHING_AWAY,
  whileAway,
  type Away,
} from "./awayCounts";

const at = (iso: string) => Date.parse(iso);
const AWAY: Away[] = [{ from: at("2026-10-09T10:00:00Z"), to: at("2026-10-09T11:00:00Z") }];

function finished(id: string, name: string, more: Partial<FinishedTask> = {}): FinishedTask {
  return {
    id,
    asker: 1,
    name,
    chat: null,
    persona: "devops",
    how: "done",
    outcome: "done",
    folds: true,
    report: "",
    changed: null,
    ended: "2026-10-09T10:30:00Z",
    place: "alpha",
    branch: null,
    reopens: true,
    not_reopened: null,
    did_not_start: false,
    attempts: 0,
    waits: null,
    ...more,
  };
}

const names: Record<number, string> = { 1: "steward 1", 2: "steward 2", 4: "talk" };
const nameOf = (session: number) => names[session] ?? String(session);

function summary(
  tasks: FinishedTask[],
  cameToNeed: number[] = [],
  states: Partial<ChatStates> = {},
  away: Away[] = AWAY,
) {
  return awaySummaryOf({
    away,
    finished: tasks,
    cameToNeed: new Set(cameToNeed),
    states: { ...nothingKnown, ...states },
    nameOf,
  });
}

describe("what a summary counts (#1514)", () => {
  it("counts only what ended while the person was away", () => {
    const counted = summary([
      finished("A", "before", { ended: "2026-10-09T09:59:59Z" }),
      finished("B", "while", { ended: "2026-10-09T10:00:00Z" }),
      finished("C", "while too", { ended: "2026-10-09T11:00:00Z" }),
      finished("D", "after", { ended: "2026-10-09T11:00:01Z" }),
      finished("E", "no time", { ended: null }),
      finished("F", "not a time", { ended: "yesterday" }),
    ]);
    expect(counted.done.map((one) => one.key)).toEqual(["task:B", "task:C"]);
  });

  it("counts a task by how it ended, as every surface counts it", () => {
    const counted = summary([
      finished("A", "done"),
      finished("B", "cancelled", { how: "cancelled" }),
      finished("C", "stopped", { how: "stopped_by_person" }),
      finished("D", "failed", { how: "failed" }),
      finished("E", "blocked", { how: "blocked" }),
      finished("F", "unreported", { how: "unreported" }),
    ]);
    expect(counted.done.map((one) => one.key)).toEqual(["task:A", "task:B", "task:C"]);
    expect(counted.failed.map((one) => one.key)).toEqual(["task:D", "task:E", "task:F"]);
  });

  it("goes to a finished task's row, named with the chat that asked for it", () => {
    const task = finished("A", "lint", { asker: 2 });
    expect(summary([task]).done).toEqual([
      { key: "task:A", says: "lint, a task of steward 2", go: { to: "finished", task } },
    ]);
  });

  it("counts a chat as waiting only when it came to need you while away and still does", () => {
    const counted = summary([], [4, 2], { needsYou: [1, 2, 4] });
    // 1 was waiting before the person left; 2 and 4 came while away, in the queue's order.
    expect(counted.waiting).toEqual([
      { key: "chat:2", says: "steward 2", go: { to: "chat", session: 2 } },
      { key: "chat:4", says: "talk", go: { to: "chat", session: 4 } },
    ]);
    // Answered since: no longer waiting.
    expect(summary([], [4, 2], { needsYou: [1, 4] }).waiting.map((one) => one.key)).toEqual([
      "chat:4",
    ]);
  });

  it("counts a chat there only for a task that came to nothing once, as the failure", () => {
    const failedTasks = { 1: [{ id: "D", task: "lint", chat: null }] };
    const needs = { 1: ["lint failed"] };
    expect(summary([], [1], { needsYou: [1], failedTasks, needs }).waiting).toEqual([]);
    // With a reason of its own besides, it is waiting on you too.
    expect(
      summary([], [1], { needsYou: [1], failedTasks, needs, bySession: { 1: "waiting" } }).waiting,
    ).toHaveLength(1);
    expect(
      summary([], [1], { needsYou: [1], failedTasks, needs, reports: { 1: ["talk"] } }).waiting,
    ).toHaveLength(1);
  });

  it("tells a failure's sentence from another reason by what it says, not by how many", () => {
    const failedTasks = {
      1: [
        { id: "D", task: "lint", chat: null },
        { id: "E", task: "build", chat: null },
      ],
    };
    // Both failures, each said its own way.
    const both = ["lint ended without a report", "build did not start: no profile"];
    expect(summary([], [1], { needsYou: [1], failedTasks, needs: { 1: both } }).waiting).toEqual(
      [],
    );
    // One reason of another kind, fewer sentences than failures: waiting on you.
    const other = ["its report has nowhere to go because steward 2 has closed"];
    expect(
      summary([], [1], { needsYou: [1], failedTasks, needs: { 1: other } }).waiting,
    ).toHaveLength(1);
    expect(
      summary([], [1], { needsYou: [1], failedTasks, needs: { 1: [...both, ...other] } }).waiting,
    ).toHaveLength(1);
  });

  it("counts a dispatch refused while nobody was there by when it was last refused (#1551)", () => {
    const refusal = (asking: string, latest: string): AwayRefusal => ({
      asking,
      target: "devops",
      workspace: "alpha",
      latest: at(latest) / 1000,
      times: 2,
      allows: "",
      nowhere: null,
      shown: "",
    });
    const counted = awaySummaryOf({
      away: AWAY,
      finished: [],
      cameToNeed: new Set(),
      states: nothingKnown,
      nameOf,
      refusedAway: [
        refusal("steward", "2026-10-09T10:20:00Z"),
        refusal("lead", "2026-10-09T09:20:00Z"),
      ],
    });
    // Two personas and nothing a chat wrote; it goes to the list where it is answered.
    expect(counted.refused).toEqual([
      {
        key: "refused:steward:devops:alpha",
        says: "steward wanted devops",
        go: { to: "needs-you" },
      },
    ]);
  });

  it("counts nothing when the person was never away", () => {
    expect(summary([finished("A", "x")], [1], { needsYou: [1] }, [])).toBe(NOTHING_AWAY);
  });

  it("holds several times away", () => {
    const twice: Away[] = [
      { from: 0, to: 10 },
      { from: 20, to: 30 },
    ];
    expect([5, 15, 25, 31].map((t) => whileAway(twice, t))).toEqual([true, false, true, false]);
  });
});

describe("which chats came to need the person while away (#1514, #1551)", () => {
  const states = (more: Partial<ChatStates>): ChatStates => ({ ...nothingKnown, ...more });

  it("counts a chat that came into the queue, and none that left it", () => {
    const was = states({ needsYou: [1], bySession: { 1: "waiting" }, movedAt: { 1: 3 } });
    const now = states({
      needsYou: [2],
      bySession: { 1: "running", 2: "waiting" },
      movedAt: { 1: 5, 2: 6 },
    });
    expect(cameToNeedOf(was, now)).toEqual([2]);
  });

  it("counts a chat already in the queue that has something new for the person", () => {
    // 1 was waiting on a failure's look, and has since asked a question of its own; 2 was
    // waiting on its turn and its tasks' report has since come with nowhere to go; 3 is as
    // it was.
    const was = states({
      needsYou: [1, 2, 3],
      bySession: { 1: "running", 2: "waiting", 3: "waiting" },
      movedAt: { 1: 1, 2: 2, 3: 3 },
      needs: { 1: ["check staging failed"], 3: ["Its report has nowhere to go"] },
    });
    const now = states({
      needsYou: [1, 2, 3],
      bySession: { 1: "waiting", 2: "waiting", 3: "waiting" },
      movedAt: { 1: 9, 2: 2, 3: 3 },
      needs: {
        1: ["check staging failed"],
        2: ["Its report has nowhere to go"],
        3: ["Its report has nowhere to go"],
      },
    });
    expect(cameToNeedOf(was, now)).toEqual([1, 2]);
  });

  it("counts a chat that answered and asked again while away, though it waits as it did", () => {
    const was = states({ needsYou: [1], bySession: { 1: "waiting" }, movedAt: { 1: 3 } });
    const now = states({ needsYou: [1], bySession: { 1: "waiting" }, movedAt: { 1: 8 } });
    expect(cameToNeedOf(was, now)).toEqual([1]);
  });

  it("counts a second item of the same words as new", () => {
    const was = states({ needsYou: [1], refusals: { 1: ["a commit"] } });
    const now = states({ needsYou: [1], refusals: { 1: ["a commit", "a commit"] } });
    expect(cameToNeedOf(was, now)).toEqual([1]);
  });
});

describe("what a summary says (#1514)", () => {
  const items = (n: number) =>
    Array.from({ length: n }, (_, i) => ({
      key: String(i),
      says: "",
      go: { to: "chat" as const, session: i },
    }));

  it("says the spec's sentence", () => {
    const counted = { done: items(7), failed: items(1), waiting: items(2), refused: [] };
    expect(awaySaid(counted)).toBe("While you were away: 7 tasks done, 1 failed, 2 waiting on you");
    expect(countedAway(counted)).toBe(10);
  });

  it("names what it counts in its first part, and leaves out a part with none", () => {
    expect(awayPartsSaid({ done: items(1), failed: [], waiting: [], refused: [] })).toEqual([
      { part: "done", says: "1 task done" },
    ]);
    expect(awaySaid({ done: [], failed: items(2), waiting: items(1), refused: [] })).toBe(
      "While you were away: 2 tasks failed, 1 waiting on you",
    );
    expect(awaySaid({ done: [], failed: [], waiting: items(1), refused: [] })).toBe(
      "While you were away: 1 chat waiting on you",
    );
    expect(awaySaid({ done: [], failed: [], waiting: items(3), refused: [] })).toBe(
      "While you were away: 3 chats waiting on you",
    );
    expect(awaySaid(NOTHING_AWAY)).toBe("");
  });

  it("says refused dispatches last, always as dispatches (#1551)", () => {
    expect(awaySaid({ done: items(2), failed: [], waiting: [], refused: items(1) })).toBe(
      "While you were away: 2 tasks done, 1 dispatch refused",
    );
    expect(awaySaid({ done: [], failed: [], waiting: [], refused: items(3) })).toBe(
      "While you were away: 3 dispatches refused",
    );
  });
});
