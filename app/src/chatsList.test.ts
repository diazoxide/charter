import { describe, expect, it } from "vitest";
import type { FinishedTask } from "./bindings";
import {
  arranged,
  countsOf,
  filters,
  found,
  isLive,
  liveBelow,
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

  it("is each one with a chat under it that is not over", () => {
    expect(liveBelow(rows, doing({ 2: "done", 3: "working", 5: "failed" }))).toEqual([1]);
    expect(liveBelow(rows, doing({ 2: "done", 3: "cancelled", 5: "needs-you" }))).toEqual([4]);
  });

  it("counts an idle chat as not over, and every end as over", () => {
    expect(isLive("idle")).toBe(true);
    expect(isLive(undefined)).toBe(true);
    for (const kind of ["done", "failed", "cancelled", "unreported", "reported"] as const)
      expect(isLive(kind), kind).toBe(false);
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
      outcome: "blocked",
      folds: false,
      report: "",
      changed: null,
      ended: null,
      place: "beta",
      branch: null,
      reopens: false,
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
    outcome,
    folds: outcome === "done" || outcome === "cancelled",
    report: "",
    changed: null,
    ended: null,
    place: "alpha",
    branch: null,
    reopens: false,
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
