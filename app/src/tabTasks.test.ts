import { describe, expect, it } from "vitest";
import { nothingKnown, type ChatStates, type State } from "./chatState";
import type { ChatRow } from "./chatsTree";
import type { Shown } from "./shownState";
import {
  chipSaid,
  countsOf,
  hasTasks,
  kindsOf,
  menuOf,
  needsSaid,
  neighbour,
  wearsChip,
  type Ended,
} from "./tabTasks";

/** A row of a tab's tree: the session's own chat at level 1, a task below it. */
function row(session: number, level: number, more: Partial<ChatRow> = {}): ChatRow {
  return {
    session,
    name: level === 1 ? `steward ${session}` : `task ${session}`,
    persona: "steward",
    workspace: "alpha",
    shell: false,
    parent: level === 1 ? null : 1,
    mode: level === 1 ? null : "task",
    from: level === 1 ? null : "steward 1",
    tab: level === 1,
    branch: null,
    report: level === 1 ? null : "owed",
    outcome: null,
    asking: null,
    harness: "Claude Code",
    level,
    posinset: 1,
    setsize: 1,
    orphaned: false,
    ...more,
  };
}

function states(by: Record<number, State>, needsYou: number[] = []): ChatStates {
  return { ...nothingKnown, bySession: by, needsYou };
}

const DONE: Shown = { kind: "done", word: "done", shape: "tick", token: "text.muted" };
const FAILED: Shown = { kind: "failed", word: "failed", shape: "cross", token: "state.failed" };
const CANCELLED: Shown = {
  kind: "cancelled",
  word: "cancelled",
  shape: "dash",
  token: "text.muted",
};

/** A task that has ended and has a line. */
function ended(key: string, asker: number, shown: Shown, more: Partial<Ended> = {}): Ended {
  return {
    key,
    asker,
    name: key,
    persona: null,
    shown,
    folds: shown.kind === "done" || shown.kind === "cancelled",
    elsewhere: null,
    ...more,
  };
}

describe("what a tab's chip counts", () => {
  it("counts the tasks by how they stand, and never the session's own chat", () => {
    const rows = [
      row(1, 1),
      row(2, 2),
      row(3, 2),
      row(4, 2, { report: "sent", outcome: "done" }),
      row(5, 2, { report: "sent", outcome: "failed" }),
      row(6, 2, { report: "failed", outcome: null }),
      row(7, 2, { report: "sent", outcome: "cancelled" }),
      row(8, 2),
    ];
    const kinds = kindsOf(
      states(
        { 1: "running", 2: "running", 3: "waiting", 4: "waiting", 5: "waiting", 8: "waiting" },
        [3],
      ),
      rows,
    );

    // One working; two waiting (one needs you, one idle); two failed (one of them ended
    // without a report); two finished (done and cancelled).
    expect(countsOf(rows, kinds, [])).toEqual({ working: 1, waiting: 2, failed: 2, done: 2 });
  });

  it("counts a task that ended as the core folds it, not as its state reads", () => {
    const lines = [
      ended("talk", 1, DONE),
      ended("probe", 1, FAILED),
      // Closed by the person: it reads cancelled, and the core does not fold it.
      ended("halt", 1, CANCELLED, { folds: false }),
    ];

    expect(countsOf([row(1, 1)], [undefined], lines)).toEqual({
      working: 0,
      waiting: 0,
      failed: 2,
      done: 1,
    });
  });

  it("says the counts in words, and nothing for a zero", () => {
    expect(chipSaid("steward 4", { working: 2, waiting: 1, failed: 0, done: 3 })).toBe(
      "Tasks of steward 4: 2 working, 1 waiting, 3 done",
    );
    expect(chipSaid("steward 4", { working: 0, waiting: 0, failed: 1, done: 0 })).toBe(
      "Tasks of steward 4: 1 failed",
    );
  });

  it("is worn for an open task, an ended one, or a chat waiting off screen, and for nothing else", () => {
    expect(wearsChip([row(1, 1)], [], [])).toBe(false);
    expect(wearsChip([row(1, 1), row(2, 2)], [], [])).toBe(true);
    expect(wearsChip([row(1, 1)], [ended("talk", 1, DONE)], [])).toBe(true);
    expect(wearsChip([row(1, 1)], [], [{ session: 1, name: "steward 1" }])).toBe(true);
    // A hand alone is no task to count or to list.
    expect(hasTasks([row(1, 1)], [])).toBe(false);
    expect(hasTasks([row(1, 1)], [ended("talk", 1, DONE)])).toBe(true);
  });

  it("says who waits, and how many more", () => {
    expect(needsSaid(["sweep"])).toBe("sweep needs you");
    expect(needsSaid(["sweep", "talk", "probe"])).toBe("sweep and 2 more need you");
  });
});

describe("what a tab's menu lists", () => {
  const rows = [
    row(1, 1),
    row(2, 2),
    row(3, 3, { parent: 2, workspace: "beta" }),
    row(4, 2, { report: "sent", outcome: "done" }),
    row(5, 2, { report: "sent", outcome: "cancelled" }),
    row(6, 2, { report: "sent", outcome: "failed" }),
  ];
  const kinds = kindsOf(states({ 1: "waiting", 2: "running", 3: "running" }), rows);

  it("lists the session's own chat first, then its tasks by who asked whom", () => {
    const menu = menuOf(rows, kinds, [], 1);

    expect(menu.lines.map((line) => [line.session, line.level])).toEqual([
      [1, 1],
      [2, 2],
      [3, 3],
      [6, 2],
    ]);
    expect(menu.lines[0].current).toBe(true);
    expect(menu.lines.filter((line) => line.current)).toHaveLength(1);
  });

  it("says in words who asked for a task that the session did not ask for itself", () => {
    const menu = menuOf(rows, kinds, [], 1);

    expect(menu.lines.find((line) => line.session === 3)?.askedBy).toBe("task 2");
    expect(menu.lines.find((line) => line.session === 2)?.askedBy).toBeNull();
  });

  it("folds done and cancelled, and leaves a failure a row of its own", () => {
    const menu = menuOf(rows, kinds, [], 1);

    expect(menu.finished.map((line) => line.session)).toEqual([4, 5]);
    expect(menu.lines.map((line) => line.session)).toContain(6);
  });

  it("says the workspace of a task that works in another one than its session", () => {
    const menu = menuOf(rows, kinds, [], 1);

    expect(menu.lines.find((line) => line.session === 3)?.elsewhere).toBe("beta");
    expect(menu.lines.find((line) => line.session === 2)?.elsewhere).toBeNull();
  });

  it("keeps a finished task out of the fold while a task it asked for is still listed", () => {
    const nested = [
      row(1, 1),
      row(2, 2, { report: "sent", outcome: "done" }),
      row(3, 3, { parent: 2 }),
    ];
    const menu = menuOf(nested, kindsOf(states({ 3: "running" }), nested), [], 1);

    expect(menu.lines.map((line) => line.session)).toEqual([1, 2, 3]);
    expect(menu.finished).toEqual([]);
  });

  it("folds a finished task with the tasks under it once they have all finished, and keeps who asked", () => {
    const nested = [
      row(1, 1),
      row(2, 2, { report: "sent", outcome: "done" }),
      row(3, 3, { parent: 2, report: "sent", outcome: "done" }),
      row(4, 4, { parent: 3, report: "sent", outcome: "cancelled" }),
    ];
    const menu = menuOf(nested, kindsOf(states({}), nested), [], 1);

    expect(menu.lines.map((line) => line.session)).toEqual([1]);
    expect(menu.finished.map((line) => [line.session, line.level, line.askedBy])).toEqual([
      [2, 2, null],
      [3, 2, "task 2"],
      [4, 2, "task 3"],
    ]);
  });

  it("keeps a finished task out of the fold while a failure hangs under it", () => {
    const nested = [row(1, 1), row(2, 2, { report: "sent", outcome: "done" })];
    const menu = menuOf(nested, kindsOf(states({}), nested), [ended("probe", 2, FAILED)], 1);

    expect(menu.lines.map((line) => line.name)).toEqual(["steward 1", "task 2", "probe"]);
    expect(menu.lines[2]).toMatchObject({ level: 3, askedBy: "task 2", ended: FAILED });
    expect(menu.finished).toEqual([]);
  });

  it("puts an ended task under the chat that asked for it, after that chat's open tasks", () => {
    const lines = [
      ended("probe", 1, FAILED, { report: "The deploy is red." }),
      ended("notes", 2, FAILED, { elsewhere: "beta" }),
      ended("old", 1, DONE),
    ];
    const menu = menuOf(rows, kinds, lines, 1);

    expect(menu.lines.map((line) => line.name)).toEqual([
      "steward 1",
      "task 2",
      "task 3",
      "notes",
      "task 6",
      "probe",
    ]);
    expect(menu.lines.find((line) => line.name === "notes")).toMatchObject({
      level: 3,
      askedBy: "task 2",
      elsewhere: "beta",
    });
    expect(menu.lines.find((line) => line.name === "probe")).toMatchObject({
      level: 2,
      report: "The deploy is red.",
      session: undefined,
    });
    expect(menu.finished.map((line) => line.name)).toEqual(["task 4", "task 5", "old"]);
  });

  it("never folds an ended task the core does not fold, whatever its state reads", () => {
    const menu = menuOf(
      [row(1, 1)],
      [undefined],
      [ended("halt", 1, CANCELLED, { folds: false, qualifier: "closed by the person" })],
      1,
    );

    expect(menu.finished).toEqual([]);
    expect(menu.lines[1]).toMatchObject({ name: "halt", qualifier: "closed by the person" });
  });

  it("marks the line of a task that ended and is still what the tab shows", () => {
    const lines = [ended("talk", 1, FAILED, { session: 9 }), ended("probe", 1, DONE)];
    const menu = menuOf([row(1, 1)], [undefined], lines, 9);

    const line = menu.lines.find((one) => one.session === 9);
    expect(line).toMatchObject({ name: "talk", current: true, level: 2, ended: FAILED });
    expect(menu.lines[0].current).toBe(false);
    expect(menu.finished.map((one) => one.name)).toEqual(["probe"]);
  });

  it("still lists an ended task whose asker is not a chat of the tab any more", () => {
    const menu = menuOf([row(1, 1)], [undefined], [ended("orphan", 77, FAILED)], 1);

    expect(menu.lines.map((line) => [line.name, line.level])).toEqual([
      ["steward 1", 1],
      ["orphan", 2],
    ]);
  });
});

describe("the next and the previous chat in a tab", () => {
  const rows = [row(1, 1), row(2, 2), row(3, 2)];

  it("are the neighbours in the menu's order, round the ends", () => {
    expect(neighbour(rows, 1, 1)).toBe(2);
    expect(neighbour(rows, 3, 1)).toBe(1);
    expect(neighbour(rows, 1, -1)).toBe(3);
    expect(neighbour(rows, 2, -1)).toBe(1);
  });

  it("start from the ends when the tab shows a chat that is not listed any more", () => {
    expect(neighbour(rows, 9, 1)).toBe(1);
    expect(neighbour(rows, 9, -1)).toBe(3);
  });

  it("are nothing in a tab with one chat", () => {
    expect(neighbour([row(1, 1)], 1, 1)).toBeUndefined();
  });
});
