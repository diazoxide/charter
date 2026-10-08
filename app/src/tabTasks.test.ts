import { describe, expect, it } from "vitest";
import { nothingKnown, type ChatStates, type State } from "./chatState";
import type { ChatRow } from "./chatsTree";
import type { Shown } from "./shownState";
import {
  chipSaid,
  countsOf,
  kindsOf,
  menuOf,
  neighbour,
  sinceClock,
  sinceSaid,
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
    ];
    const kinds = kindsOf(
      states({ 1: "running", 2: "running", 3: "waiting", 4: "waiting", 5: "waiting" }, [3]),
      rows,
    );

    // Two have not ended (one working, one that needs you), two failed (one of them ended
    // without a report), two finished (done and cancelled).
    expect(countsOf(rows, kinds, [])).toEqual({ working: 2, failed: 2, done: 2 });
  });

  it("counts a task that ended and is still on a line", () => {
    const ended: Ended[] = [
      { key: "e9", session: 9, asker: 1, name: "talk", persona: null, shown: DONE },
      { key: "e8", asker: 1, name: "probe", persona: null, shown: FAILED },
    ];
    expect(countsOf([row(1, 1)], [undefined], ended)).toEqual({ working: 0, failed: 1, done: 1 });
  });

  it("says the counts in words, and nothing for a zero", () => {
    expect(chipSaid("steward 4", { working: 2, failed: 0, done: 3 })).toBe(
      "Tasks of steward 4: 2 working, 3 done",
    );
    expect(chipSaid("steward 4", { working: 0, failed: 1, done: 0 })).toBe(
      "Tasks of steward 4: 1 failed",
    );
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

  it("has a line for a task that ended and is still what the tab shows", () => {
    const ended: Ended[] = [
      { key: "e9", session: 9, asker: 1, name: "talk", persona: "devops", shown: FAILED },
      { key: "e8", session: 8, asker: 1, name: "probe", persona: null, shown: DONE },
    ];
    const menu = menuOf([row(1, 1)], [undefined], ended, 9);

    const line = menu.lines.find((one) => one.session === 9);
    expect(line).toMatchObject({ name: "talk", current: true, level: 2, ended: FAILED });
    expect(menu.lines[0].current).toBe(false);
    expect(menu.finished.map((one) => one.name)).toEqual(["probe"]);
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

describe("how long a chat has been in its state, as this window saw it", () => {
  it("says no time for a state the window did not see begin", () => {
    const clock = sinceClock();
    clock.read(new Map([[2, "working"]]), 1_000);

    expect(clock.since(2)).toBeNull();
  });

  it("times a state from the change the window saw, and a chat from its arrival", () => {
    const clock = sinceClock();
    clock.read(new Map([[2, "working"]]), 1_000);
    clock.read(
      new Map([
        [2, "done"],
        [3, "working"],
      ]),
      5_000,
    );

    expect(clock.since(2)).toBe(5_000);
    expect(clock.since(3)).toBe(5_000);
  });

  it("does not time the first word about a chat nothing had been heard from", () => {
    const clock = sinceClock();
    clock.read(new Map([[2, "unheard"]]), 1_000);
    clock.read(new Map([[2, "idle"]]), 2_000);

    expect(clock.since(2)).toBeNull();
  });

  it("tells its readers when a time changed, and forgets a chat that is gone", () => {
    const clock = sinceClock();
    let told = 0;
    clock.subscribe(() => (told += 1));
    clock.read(new Map([[2, "working"]]), 1_000);
    clock.read(new Map([[2, "working"]]), 2_000);
    expect(told).toBe(0);

    clock.read(new Map([[2, "done"]]), 3_000);
    expect(told).toBe(1);

    clock.read(new Map(), 4_000);
    expect(clock.since(2)).toBeNull();
  });

  it("says a time a person reads", () => {
    expect(sinceSaid(0)).toBe("now");
    // Built, not written: a number with an `s` after it reads as a motion time to the guard
    // that keeps those in the theme (`theme/literals.test.ts`).
    expect(sinceSaid(59)).toBe(["59", "s"].join(""));
    expect(sinceSaid(60)).toBe("1m");
    expect(sinceSaid(3_599)).toBe("59m");
    expect(sinceSaid(3_600)).toBe("1h");
    expect(sinceSaid(26 * 3_600)).toBe("1d");
  });
});
