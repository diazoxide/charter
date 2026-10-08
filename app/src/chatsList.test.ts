import { describe, expect, it } from "vitest";
import type { FinishedTask } from "./bindings";
import {
  arranged,
  countsOf,
  filters,
  found,
  isLive,
  liveBelow,
  overBelow,
  overOf,
  matches,
  matchesFinished,
  rankOf,
  sessionOrder,
  sinceSaid,
  stamped,
  summaryOf,
  type Rank,
} from "./chatsList";
import { chatsTree, type ChatRow, type ListedChat } from "./chatsTree";
import type { Shown, ShownKind } from "./shownState";

/** A chat started by `parent`, where one started it, working in `workspace`. */
function listed(
  session: number,
  parent: number | null = null,
  workspace = "alpha",
  more: Partial<ListedChat> = {},
): ListedChat {
  return {
    session,
    name: `chat ${session}`,
    persona: null,
    workspace,
    shell: false,
    parent,
    mode: parent === null ? null : "task",
    from: parent === null ? null : `chat ${parent}`,
    tab: parent === null,
    branch: null,
    report: null,
    outcome: null,
    asking: null,
    harness: null,
    ...more,
  };
}

/** What each chat is doing, by number; a chat that is not named is idle. */
const doing =
  (kinds: Record<number, ShownKind>) =>
  (row: ChatRow): ShownKind =>
    kinds[row.session] ?? "idle";

const numbers = (rows: readonly ChatRow[]) => rows.map((row) => row.session);

describe("the order the sessions stand in (V100-47)", () => {
  const rows = chatsTree([listed(1), listed(2), listed(3), listed(4), listed(5, 2), listed(6, 2)]);

  it("is needs you, then working, then the rest, each in the order it was started", () => {
    const kinds = doing({ 1: "idle", 2: "working", 3: "needs-you", 4: "working" });

    expect(sessionOrder(rows, kinds, false)).toEqual([3, 2, 4, 1]);
  });

  it("puts a session where the most urgent chat under it would stand", () => {
    expect(sessionOrder(rows, doing({ 6: "needs-you", 3: "working" }), false)).toEqual([
      2, 3, 1, 4,
    ]);
  });

  it("counts a task asking its asker, and a chat nothing is heard from, as at work", () => {
    expect([rankOf("asking"), rankOf("unheard"), rankOf("working")]).toEqual([1, 1, 1]);
    expect([rankOf("idle"), rankOf("done"), rankOf(undefined)]).toEqual([2, 2, 2]);
    expect(rankOf("needs-you")).toBe(0);
  });

  it("keeps the chats under a session in the order they were started, whatever they do", () => {
    const order = sessionOrder(rows, doing({ 6: "needs-you" }), false);

    expect(numbers(arranged(rows, order))).toEqual([2, 5, 6, 1, 3, 4]);
  });

  it("groups by workspace, the workspaces by name, with the same order inside each", () => {
    const mixed = chatsTree([
      listed(1, null, "beta"),
      listed(2, null, "alpha"),
      listed(3, null, "beta"),
      listed(4, null, "alpha"),
    ]);

    expect(sessionOrder(mixed, doing({ 3: "needs-you", 4: "working" }), true)).toEqual([
      4, 2, 3, 1,
    ]);
  });
});

describe("rows put in an order", () => {
  const rows = chatsTree([listed(1), listed(2, 1), listed(3), listed(4)]);

  it("says where each row stands among the rows now beside it", () => {
    const moved = stamped(arranged(rows, [4, 1, 3]));

    expect(moved.map((row) => `${row.session} ${row.posinset}/${row.setsize}`)).toEqual([
      "4 1/3",
      "1 2/3",
      "2 1/1",
      "3 3/3",
    ]);
  });

  it("puts a session that arrived after the order was taken after the ones it names", () => {
    expect(numbers(arranged(rows, [3, 1]))).toEqual([3, 1, 2, 4]);
  });

  it("leaves a chat whose parent closed where it was drawn, under an order that does not name it", () => {
    // 1 started 2, which started 3; 2 has closed, so 3 stands at the top, marked.
    const orphaned = chatsTree([listed(1), listed(3, 2), listed(4)]);

    expect(numbers(arranged(orphaned, [1, 4]))).toEqual([1, 3, 4]);
    expect(numbers(arranged(orphaned, [4, 1]))).toEqual([4, 1, 3]);
    // Its parent stood at the top: it takes that place.
    expect(numbers(arranged(orphaned, [2, 4, 1]))).toEqual([3, 4, 1]);
  });
});

describe("which sessions are open by themselves (V100-48)", () => {
  const rows = chatsTree([listed(1), listed(2, 1), listed(3, 1), listed(4), listed(5, 4)]);

  it("is each one with a chat under it that is not done and not cancelled", () => {
    expect(liveBelow(rows, doing({ 2: "done", 3: "working", 5: "done" }))).toEqual([1]);
    expect(liveBelow(rows, doing({ 2: "done", 3: "cancelled", 5: "needs-you" }))).toEqual([4]);
  });

  it("keeps a session open over an open task that failed, went without a report or only reported", () => {
    // A blocked task reads failed and is not ended (#1485): it is waiting on something, and
    // is never folded away by the list itself.
    for (const kind of ["failed", "unreported", "reported"] as const)
      expect(liveBelow(rows, doing({ 2: "done", 3: "done", 5: kind })), kind).toEqual([4]);
  });

  it("folds an open chat away only when it is done or cancelled", () => {
    expect(isLive("idle")).toBe(true);
    expect(isLive(undefined)).toBe(true);
    for (const kind of ["done", "cancelled"] as const) expect(isLive(kind), kind).toBe(false);
    for (const kind of ["failed", "unreported", "reported"] as const)
      expect(isLive(kind), kind).toBe(true);
  });
});

describe("the filter (V100-49)", () => {
  const row = chatsTree([
    listed(7, null, "beta", { name: "live check talk", persona: "devops" }),
  ])[0];
  const working: Shown = {
    kind: "working",
    word: "working",
    shape: "ring",
    token: "state.running",
  };
  const asks = (text: string, ranks: Rank[] = []) => matches(row, working, { text, ranks });

  it("finds a chat by its name, its persona, its workspace or its state's word", () => {
    expect(asks("check")).toBe(true);
    expect(asks("devops")).toBe(true);
    expect(asks("beta")).toBe(true);
    expect(asks("working")).toBe(true);
    expect(asks("steward")).toBe(false);
  });

  it("asks for every word typed, in any of the four, in any case", () => {
    expect(asks("DEVOPS beta")).toBe(true);
    expect(asks("devops alpha")).toBe(false);
  });

  it("finds a name whatever accents it was typed or written with", () => {
    const jose = chatsTree([listed(8, null, "alpha", { name: "José's review" })])[0];

    expect(matches(jose, working, { text: "jose", ranks: [] })).toBe(true);
    expect(matches(row, working, { text: "chéck", ranks: [] })).toBe(true);
  });

  it("asks, with a chip, for a chat whose state stands where the chip's does in the order", () => {
    expect(asks("", [1])).toBe(true);
    expect(asks("", [0])).toBe(false);
    expect(asks("", [0, 1])).toBe(true);
    expect(asks("steward", [1])).toBe(false);
    // What the order counts as at work, the "working" chip finds: a chat nothing is heard
    // from, and a task asking the chat that dispatched it.
    const unheard: Shown = {
      kind: "unheard",
      word: "running (no detail from claude)",
      shape: working.shape,
      token: "text.muted",
    };
    const asking: Shown = { ...working, kind: "asking", word: "asking steward 1" };
    expect(matches(row, unheard, { text: "", ranks: [1] })).toBe(true);
    expect(matches(row, asking, { text: "", ranks: [1] })).toBe(true);
    expect(matches(row, undefined, { text: "", ranks: [1] })).toBe(false);
  });

  it("asks for nothing while nothing is typed and no chip is pressed", () => {
    expect(filters({ text: "  ", ranks: [] })).toBe(false);
    expect(filters({ text: "a", ranks: [] })).toBe(true);
    expect(filters({ text: "", ranks: [1] })).toBe(true);
  });

  it("finds a finished task by its name, persona, place and how it ended, and never by a chip", () => {
    const ended = {
      id: "01K6",
      asker: 1,
      name: "check staging",
      persona: "devops",
      how: "blocked" as const,
      outcome: "blocked",
      folds: false,
      report: "",
      changed: null,
      ended: null,
      place: "beta",
      branch: null,
      reopens: false,
      not_reopened: null,
      chat: null,
      did_not_start: false,
      attempts: 0,
      waits: null,
    };
    for (const text of ["staging", "devops", "beta", "failed", "blocked", "BETA check"])
      expect(matchesFinished(ended, { text, ranks: [] }), text).toBe(true);
    expect(matchesFinished(ended, { text: "alpha", ranks: [] })).toBe(false);
    expect(matchesFinished(ended, { text: "", ranks: [] })).toBe(false);
    expect(matchesFinished(ended, { text: "staging", ranks: [1] })).toBe(false);
  });

  it("keeps every row above a match, in the order they stood, and no other", () => {
    const rows = chatsTree([listed(1), listed(2, 1), listed(3, 2), listed(4, 1), listed(5)]);

    expect(numbers(found(rows, new Set([3])))).toEqual([1, 2, 3]);
    expect(numbers(found(rows, new Set([4, 5])))).toEqual([1, 4, 5]);
    expect(numbers(found(rows, new Set()))).toEqual([]);
  });
});

describe("what a folded session says of its finished tasks (V100-48)", () => {
  const task = (outcome: string): FinishedTask => ({
    id: outcome,
    asker: 4,
    name: outcome,
    persona: null,
    how: outcome === "ended without a report" ? "unreported" : (outcome as FinishedTask["how"]),
    outcome,
    folds: outcome === "done" || outcome === "cancelled",
    report: "",
    changed: null,
    ended: null,
    place: "alpha",
    branch: null,
    reopens: false,
    not_reopened: null,
    chat: null,
    did_not_start: false,
    attempts: 0,
    waits: null,
  });

  it("counts them by how each ended, in the shape and the word a row says that state in", () => {
    const summary = summaryOf(
      ["done", "done", "failed", "done", "ended without a report"].map(task),
    );

    expect(countsOf(summary ?? "")).toEqual([
      { shape: "tick", count: 3, word: "done" },
      { shape: "cross", count: 1, word: "failed" },
      { shape: "triangle", count: 1, word: "ended without a report" },
    ]);
  });

  it("says nothing for a session with none", () => {
    expect(summaryOf([])).toBeNull();
  });

  it("counts the open chats under it that are over with its finished tasks", () => {
    const rows = chatsTree([listed(1), listed(2, 1), listed(3, 1), listed(4, 3), listed(5)]);
    const shownAs = (kinds: Record<number, [ShownKind, string, string]>) => (row: ChatRow) => {
      const one = kinds[row.session];
      return one === undefined
        ? undefined
        : ({ kind: one[0], shape: one[1], word: one[2], token: "text.muted" } as Shown);
    };
    const over = overBelow(
      rows,
      shownAs({
        2: ["done", "tick", "done"],
        3: ["working", "ring", "working"],
        4: ["failed", "cross", "failed"],
      }),
    );

    // Each row with something over under it, at any depth; a chat at work is not counted.
    const open = overOf(over);
    expect([...open.keys()]).toEqual([1, 3]);
    expect(open.get(1)?.map((shown) => shown.word)).toEqual(["done", "failed"]);
    expect(open.get(3)?.map((shown) => shown.word)).toEqual(["failed"]);

    // With one finished task of its own that came out done: two done, one failed.
    expect(countsOf(summaryOf([task("done")], open.get(1)) ?? "")).toEqual([
      { shape: "tick", count: 2, word: "done" },
      { shape: "cross", count: 1, word: "failed" },
    ]);
    // And with no finished task at all, the open ones are still said.
    expect(countsOf(summaryOf([], open.get(3)) ?? "")).toEqual([
      { shape: "cross", count: 1, word: "failed" },
    ]);
  });
});

describe("how long a chat has been in its state", () => {
  it("is said in the largest whole unit", () => {
    expect([0, 59, 60, 3599, 3600, 86_399, 86_400 * 3].map(sinceSaid)).toEqual([
      "just now",
      "just now",
      "1m",
      "59m",
      "1h",
      "23h",
      "3d",
    ]);
  });
});
