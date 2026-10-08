import { describe, expect, it } from "vitest";
import { moved, nothingKnown, type ChatStates, type State } from "./chatState";
import type { ListedChat } from "./chatsTree";
import {
  belowOf,
  countParts,
  firstNeeding,
  helperShown,
  helpersCountOf,
  helpersCountSaid,
  helpersOf,
  helpersSaid,
  housed,
  sameHelpers,
  shortIds,
  tasksSaid,
} from "./explorerTasks";
import { totalOf } from "./taskBuckets";
import { taskCountOf } from "./taskCounts";

function listed(session: number, on: Partial<ListedChat> = {}): ListedChat {
  return {
    session,
    name: `chat ${session}`,
    persona: null,
    workspace: "alpha",
    shell: false,
    parent: null,
    mode: null,
    from: null,
    tab: true,
    branch: null,
    report: null,
    outcome: null,
    asking: null,
    harness: "claude",
    ...on,
  };
}

/** A task of `asker` with no tab of its own. */
const task = (session: number, asker: number, on: Partial<ListedChat> = {}) =>
  listed(session, { parent: asker, mode: "task", tab: false, report: "owed", ...on });

/** What the board says of each chat, and who is in the queue. */
function board(states: Record<number, State>, queue: number[] = []): ChatStates {
  return Object.entries(states).reduce(
    (known, [session, state], at) =>
      moved(known, {
        plane: "/p",
        session: Number(session),
        state,
        needs_you: queue.includes(Number(session)),
        queue,
        moved_at: at + 1,
        reports: [],
        refusals: [],
        sequence: at + 1,
        children: [],
      }),
    nothingKnown,
  );
}

describe("which chats live inside another chat's tab", () => {
  it("is every task with no tab of its own, under the session that asked, at any depth", () => {
    const house = housed([listed(1), task(2, 1), task(3, 2), task(4, 1), listed(5)]);

    expect([...house.hostOf]).toEqual([
      [2, 1],
      [3, 1],
      [4, 1],
    ]);
    expect(house.tasksOf.get(1)?.map((one) => one.session)).toEqual([2, 3, 4]);
    expect(house.tasksOf.has(5)).toBe(false);
  });

  it("leaves a handoff, a shell and a task with a tab of its own their rows", () => {
    const house = housed([
      listed(1),
      listed(2, { parent: 1, mode: "handoff" }),
      listed(3, { shell: true }),
      task(4, 1, { tab: true }),
      // Below the task that has its own tab: that tab is its home.
      task(5, 4),
    ]);

    expect([...house.hostOf]).toEqual([[5, 4]]);
  });

  it("leaves a task whose asking chat has closed a row, since nothing else would say it runs", () => {
    const house = housed([listed(1), task(7, 9), task(8, 7)]);

    expect(house.hostOf.has(7)).toBe(false);
    expect(house.hostOf.get(8)).toBe(7);
  });

  it("loses no chat to a record that loops", () => {
    const house = housed([task(2, 3), task(3, 2)]);

    expect(house.hostOf.size).toBe(0);
  });
});

describe("what a tasks line says", () => {
  it("counts a set of tasks working at a place by the one count, each in one bucket", () => {
    const tasks = [
      task(2, 1),
      task(3, 1, { asking: "steward 1" }),
      task(4, 1),
      task(5, 1, { report: "sent", outcome: "cancelled" }),
      task(6, 1, { report: "failed" }),
    ];
    const states = board({ 2: "running", 3: "running", 4: "waiting", 5: "waiting", 6: "done" });

    const count = taskCountOf(states, belowOf(tasks));

    expect(count).toEqual({ working: 2, waiting: 1, done: 1, failed: 1 });
    expect(totalOf(count)).toBe(5);
  });

  it("says each part in the session row's words, with the mark of its state", () => {
    expect(countParts({ working: 2, waiting: 1, done: 3, failed: 1 })).toEqual([
      { bucket: "working", said: "2 working", shape: "ring", token: "state.running" },
      { bucket: "waiting", said: "1 waiting", shape: "pause", token: "text.muted" },
      { bucket: "done", said: "3 done", shape: "tick", token: "text.muted" },
      { bucket: "failed", said: "1 failed", shape: "cross", token: "state.failed" },
    ]);
    expect(countParts({ working: 0, waiting: 0, done: 0, failed: 0 })).toEqual([]);
    expect(tasksSaid(1)).toBe("1 task");
    expect(tasksSaid(5)).toBe("5 tasks");
  });

  it("names the task that has needed you longest", () => {
    const tasks = [task(2, 1), task(3, 1)];

    expect(firstNeeding([9, 3, 2], tasks)).toBe(3);
    expect(firstNeeding([9], tasks)).toBeUndefined();
  });
});

describe("a chat's helpers", () => {
  const withHelpers = (agents: string[]) =>
    moved(nothingKnown, {
      plane: "/p",
      session: 1,
      state: "running",
      needs_you: false,
      queue: [],
      moved_at: 1,
      reports: [],
      refusals: [],
      sequence: 1,
      children: agents.map((agent) => ({ agent, state: "running" })),
    });

  it("are only a fact while folded, and their ids once unfolded", () => {
    const states = withHelpers(["a", "b"]);

    expect(helpersOf(states, [1, 2], new Set())).toEqual([{ session: 1, agents: null }]);
    expect(helpersOf(states, [1, 2], new Set([1]))).toEqual([{ session: 1, agents: ["a", "b"] }]);
  });

  it("read the same while folded however many come and go, and not once unfolded", () => {
    const two = withHelpers(["a", "b"]);
    const three = withHelpers(["a", "b", "c"]);

    expect(sameHelpers(helpersOf(two, [1], new Set()), helpersOf(three, [1], new Set()))).toBe(
      true,
    );
    expect(
      sameHelpers(helpersOf(two, [1], new Set([1])), helpersOf(three, [1], new Set([1]))),
    ).toBe(false);
    expect(
      sameHelpers(helpersOf(two, [1], new Set()), helpersOf(nothingKnown, [1], new Set())),
    ).toBe(false);
  });

  it("are counted by how they stand: an ended one stays in the total and out of the working", () => {
    const states = moved(nothingKnown, {
      plane: "/p",
      session: 1,
      state: "running",
      needs_you: false,
      queue: [],
      moved_at: 1,
      reports: [],
      refusals: [],
      sequence: 1,
      children: [
        { agent: "a", state: "running" },
        { agent: "b", state: "done" },
        { agent: "c", state: "failed" },
        { agent: "d", state: "done" },
      ],
    });

    const count = helpersCountOf(states, 1);

    expect(count).toEqual({ total: 4, working: 1, failed: 1 });
    expect(helpersCountSaid(count)).toBe("4 helpers · 1 working · 1 failed");
    expect(helpersCountSaid({ total: 40, working: 0, failed: 0 })).toBe("40 helpers");
    expect(helpersCountOf(states, 2)).toEqual({ total: 0, working: 0, failed: 0 });
  });

  it("are counted in the singular and the plural", () => {
    expect(helpersSaid(1)).toBe("1 helper");
    expect(helpersSaid(3)).toBe("3 helpers");
  });

  it("are named by eight characters of the harness's id, and by more where two would read the same", () => {
    expect([...shortIds(["a3882da5acba68a4f00d", "thread-1", "thread-10"]).values()]).toEqual([
      "a3882da5…",
      "thread-1",
      "thread-10",
    ]);
    expect([...shortIds(["a3882da5acba68a4", "a3882da5acbaffff"]).values()]).toEqual([
      "a3882da5acba68a4",
      "a3882da5acbaffff",
    ]);
    expect([...shortIds(["a3882da5acba68a4xxxx", "a3882da5acbaffffxxxx"]).values()]).toEqual([
      "a3882da5acba68a4…",
      "a3882da5acbaffff…",
    ]);
  });

  it("say their state in the words every row uses, and an unknown word as it came", () => {
    expect(helperShown("running")).toMatchObject({ word: "working", shape: "ring" });
    expect(helperShown("done")).toMatchObject({ word: "done", shape: "tick" });
    expect(helperShown("failed")).toMatchObject({ word: "failed", shape: "cross" });
    expect(helperShown("waiting")).toMatchObject({ word: "idle" });
    expect(helperShown("paused")).toMatchObject({ word: "paused", shape: "dots" });
  });
});
