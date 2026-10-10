import { describe, expect, it } from "vitest";
import type { DispatchRow, NotStartedRow, WorktreeLoss } from "./bindings";
import {
  askersOf,
  branchSaid,
  counted,
  discardSays,
  dispatchesIn,
  EVERY_DISPATCH,
  isDispatches,
  losesNothing,
  lostSaid,
  nestedSaid,
  notStartedSaid,
  NO_PERSONA,
  NO_WORKSPACE,
  personasOf,
  saidAt,
  shownDispatches,
  waitsOnMemory,
  workspaceOf,
  workspacesOf,
  worktreeSaid,
} from "./dispatches";

function row(
  id: string,
  persona: string | null,
  asker: string,
  askerKey = `id of ${asker}`,
): DispatchRow {
  return {
    id,
    mode: "handoff",
    persona,
    task: id,
    asker,
    asker_key: askerKey,
    asker_persona: null,
    by_person: false,
    place: "alpha",
    folder: "workspaces/alpha",
    outcome: "done",
    started: "2026-10-07T12:00:00+00:00",
    ended: "2026-10-07T12:01:00+00:00",
    duration: "1m 0s",
    needed_you: 0,
    messages: 0,
    cost: null,
    tokens: null,
    brief: "",
    report: null,
    changed: null,
    open_session: null,
    session_record: null,
    worktree: null,
  };
}

const ROWS = [
  row("c", "devops", "steward 3"),
  row("b", "qa", "steward 3"),
  row("a", "devops", "planner 2"),
  row("z", null, "planner 2"),
];

const ids = (rows: DispatchRow[]) => rows.map((one) => one.id);

describe("the Dispatches tab's filters", () => {
  it("keeps every dispatch, in the order they came, until one is chosen", () => {
    expect(ids(shownDispatches(ROWS, EVERY_DISPATCH))).toEqual(["c", "b", "a", "z"]);
  });

  it("narrows to one persona, to one asking chat, and to both at once", () => {
    expect(ids(shownDispatches(ROWS, { ...EVERY_DISPATCH, persona: "devops", asker: "" }))).toEqual(
      ["c", "a"],
    );
    const planner = "id of planner 2";
    expect(ids(shownDispatches(ROWS, { ...EVERY_DISPATCH, persona: "", asker: planner }))).toEqual([
      "a",
      "z",
    ]);
    expect(
      ids(shownDispatches(ROWS, { ...EVERY_DISPATCH, persona: "devops", asker: planner })),
    ).toEqual(["a"]);
    expect(
      ids(shownDispatches(ROWS, { ...EVERY_DISPATCH, persona: "qa", asker: planner })),
    ).toEqual([]);
  });

  it("finds the dispatches that went to no persona, which no persona's name could mean", () => {
    expect(
      ids(shownDispatches(ROWS, { ...EVERY_DISPATCH, persona: NO_PERSONA, asker: "" })),
    ).toEqual(["z"]);
  });

  it("offers each persona and each asking chat once, by name", () => {
    expect(personasOf(ROWS)).toEqual(["devops", "qa", NO_PERSONA]);
    expect(personasOf(ROWS.slice(0, 3))).toEqual(["devops", "qa"]);
    expect(askersOf(ROWS)).toEqual([
      { key: "id of planner 2", name: "planner 2" },
      { key: "id of steward 3", name: "steward 3" },
    ]);
  });

  it("keeps two chats that were called the same apart, by the chat and not by its name", () => {
    const rows = [
      row("new", "devops", "steward 3", "01NEW"),
      row("old", "devops", "steward 3", "01OLD"),
    ];
    expect(askersOf(rows)).toEqual([
      { key: "01NEW", name: "steward 3" },
      { key: "01OLD", name: "steward 3" },
    ]);
    expect(ids(shownDispatches(rows, { ...EVERY_DISPATCH, persona: "", asker: "01OLD" }))).toEqual([
      "old",
    ]);
    // A name is not a key: nothing is found by it.
    expect(
      ids(shownDispatches(rows, { ...EVERY_DISPATCH, persona: "", asker: "steward 3" })),
    ).toEqual([]);
  });

  it("reads the workspace a dispatch worked in from where it worked, its branch or none", () => {
    const at = (place: string) => workspaceOf({ ...row("x", null, "steward 3"), place });
    expect(at("alpha")).toBe("alpha");
    expect(at("alpha · fix-the-queue-b5rc0def")).toBe("alpha");
    expect(at("project root")).toBe(NO_WORKSPACE);
    expect(at("project root · fix-the-queue-b5rc0def")).toBe(NO_WORKSPACE);
    // A workspace's name is `[A-Za-z0-9][A-Za-z0-9._-]*`: it never holds the separator or a
    // space, so a name that only looks like the root's word is still a workspace's.
    expect(at("project-root")).toBe("project-root");
  });

  it("narrows to one workspace, to the project root, and offers each place once", () => {
    const rows = [
      { ...row("w1", "devops", "steward 3"), place: "beta · fix-b5rc0def" },
      { ...row("w2", "qa", "steward 3"), place: "alpha" },
      { ...row("w3", "qa", "steward 3"), place: "project root" },
      { ...row("w4", "devops", "steward 3"), place: "beta" },
    ];
    expect(ids(shownDispatches(rows, { ...EVERY_DISPATCH, workspace: "beta" }))).toEqual([
      "w1",
      "w4",
    ]);
    expect(ids(shownDispatches(rows, { ...EVERY_DISPATCH, workspace: NO_WORKSPACE }))).toEqual([
      "w3",
    ]);
    expect(
      ids(shownDispatches(rows, { ...EVERY_DISPATCH, workspace: "beta", persona: "qa" })),
    ).toEqual([]);
    expect(workspacesOf(rows)).toEqual(["alpha", "beta", NO_WORKSPACE]);
    // The workspace the tab was opened from is offered though nothing was dispatched there yet.
    expect(workspacesOf(rows.slice(1, 2), "gamma")).toEqual(["alpha", "gamma"]);
    expect(workspacesOf(rows.slice(1, 2), "")).toEqual(["alpha"]);
  });

  it("starts narrowed to the workspace it was opened from, and to nothing outside one", () => {
    expect(dispatchesIn("alpha")).toEqual({ ...EVERY_DISPATCH, workspace: "alpha" });
    expect(dispatchesIn(undefined)).toEqual(EVERY_DISPATCH);
  });

  it("narrows by when a dispatch started: today, the past 7 days, or earlier", () => {
    // 2026-10-10 15:00 where the test runs: today begins at that place's midnight.
    const now = new Date(2026, 9, 10, 15, 0);
    const started = (id: string, at: Date) => ({
      ...row(id, "devops", "steward 3"),
      started: at.toISOString(),
    });
    const rows = [
      started("this-morning", new Date(2026, 9, 10, 0, 5)),
      started("last-night", new Date(2026, 9, 9, 23, 55)),
      started("six-days", new Date(2026, 9, 4, 16, 0)),
      started("eight-days", new Date(2026, 9, 2, 15, 0)),
      { ...row("unread", "devops", "steward 3"), started: "not a time" },
    ];
    const when = (when: "today" | "week" | "earlier") =>
      ids(shownDispatches(rows, { ...EVERY_DISPATCH, when }, now));
    expect(when("today")).toEqual(["this-morning"]);
    expect(when("week")).toEqual(["this-morning", "last-night", "six-days"]);
    expect(when("earlier")).toEqual(["eight-days"]);
    // Any date keeps every row, one whose time does not read as a time too.
    expect(ids(shownDispatches(rows, EVERY_DISPATCH, now))).toHaveLength(5);
  });

  it("says a record's time to the minute, and leaves alone what is not one", () => {
    expect(saidAt("2026-10-07T12:04:30+00:00")).toBe("2026-10-07 12:04 UTC");
    expect(saidAt("not a time")).toBe("not a time");
  });

  it("says how a dispatch's own branch stands, in the window's words for one", () => {
    const worktree = (standing: string) => ({
      repo: "api",
      branch: "check-the-queue-b5rc0def",
      standing,
      discard: standing === "kept",
    });
    const said = ["kept", "merged", "merged-branch-kept", "discarded", "gone"].map((standing) =>
      worktreeSaid(worktree(standing)),
    );
    expect(said).toEqual([
      // A folder that is there is kept. The row does not know whether its branch is merged: a
      // merged branch's folder stays while it holds a file.
      "own branch, folder kept",
      "own branch, merged and removed",
      "own branch, merged; folder removed, branch kept",
      "own branch, folder discarded",
      "own branch, folder removed",
    ]);
    expect(said[0]).not.toMatch(/not merged/);
    // ADR 0072 §4: on screen it is a branch and its folder, never a worktree or a piece.
    for (const one of said) expect(one).not.toMatch(/worktree|piece/i);
  });

  const LOSS: WorktreeLoss = {
    task: "check the queue",
    repo: "api",
    branch: "check-the-queue-b5rc0def",
    on: "check-the-queue-b5rc0def",
    changes: ["?? scratch.txt"],
    ignored: ["target/"],
    nested: [],
    seal: "a1b2",
    unmerged: 12,
    lost: [],
  };

  it("names an uncommitted folder that is a repository of its own, and only one", () => {
    // #1472: git lists a nested repository as one line, and its history goes with it.
    expect(nestedSaid(LOSS)).toBeUndefined();
    expect(nestedSaid({ ...LOSS, changes: ["?? vendor/lib/"], nested: ["vendor/lib/"] })).toBe(
      "1 of them is a repository of its own. Everything in it, its history too, goes with the folder:",
    );
    expect(nestedSaid({ ...LOSS, nested: ["a/", "b/"] })).toBe(
      "2 of them are repositories of their own. Everything in them, their history too, goes with the folder:",
    );
  });

  it("says what a discard removes, and that the branch purlis cut loses no commit", () => {
    const loss = LOSS;
    expect(discardSays(loss)).toBe(
      "This removes the folder of the branch check-the-queue-b5rc0def in api, which purlis cut for check the queue, for good. Nothing is merged.",
    );
    expect(discardSays({ ...loss, branch: null })).toBe(
      "This removes the folder in api, which purlis cut for check the queue, for good. Nothing is merged.",
    );
    // A branch that holds work stays; one that holds none goes only where git finds it merged.
    expect(branchSaid(loss)).toBe(
      "No commit is lost: the branch check-the-queue-b5rc0def holds 12 commits that exist nowhere else, so it stays.",
    );
    expect(branchSaid({ ...loss, unmerged: 1 })).toBe(
      "No commit is lost: the branch check-the-queue-b5rc0def holds 1 commit that exists nowhere else, so it stays.",
    );
    expect(branchSaid({ ...loss, unmerged: 0 })).toBe(
      "No commit is lost: the branch check-the-queue-b5rc0def is removed only if it is already merged.",
    );
    expect(lostSaid(loss)).toBeUndefined();
    expect(counted(1, "uncommitted file", "uncommitted files")).toBe("1 uncommitted file");
    expect(counted(3, "uncommitted file", "uncommitted files")).toBe("3 uncommitted files");
    // What a discard deletes is what is in the folder: an uncommitted file, or an ignored one.
    expect(losesNothing(loss)).toBe(false);
    expect(losesNothing({ ...loss, ignored: [] })).toBe(false);
    expect(losesNothing({ ...loss, changes: [] })).toBe(false);
    expect(losesNothing({ ...loss, changes: [], ignored: [] })).toBe(true);
  });

  it("never says no commit is lost of a folder that is on no branch", () => {
    // Its chat detached the folder and committed there: nothing keeps those commits.
    const detached: WorktreeLoss = {
      ...LOSS,
      on: null,
      changes: [],
      ignored: [],
      unmerged: 2,
      lost: ["3a823aab fix the retry", "9f00d1c2 and its test"],
    };
    expect(lostSaid(detached)).toBe("2 commits made on no branch would be lost:");
    expect(lostSaid({ ...detached, unmerged: 1 })).toBe(
      "1 commit made on no branch would be lost:",
    );
    expect(branchSaid(detached)).toBe(
      "The folder is on no branch. The branch check-the-queue-b5rc0def, which purlis cut, is removed only if it is already merged.",
    );
    expect(branchSaid(detached)).not.toMatch(/No commit is lost/);
    // And a folder that holds nothing else still loses something.
    expect(losesNothing(detached)).toBe(false);
    expect(losesNothing({ ...detached, unmerged: 0, lost: [] })).toBe(true);
  });

  it("names the branch purlis cut as the one a discard may remove, whatever the folder is on", () => {
    // Its chat switched the folder to a branch of its own. The core only ever deletes the
    // record's branch; the other stays with what is on it.
    const moved: WorktreeLoss = { ...LOSS, on: "somewhere-else", unmerged: 3 };
    expect(branchSaid(moved)).toBe(
      "The folder is on the branch somewhere-else, which stays and keeps its 3 commits that exist nowhere else. The branch check-the-queue-b5rc0def, which purlis cut, is removed only if it is already merged.",
    );
    expect(branchSaid({ ...moved, unmerged: 0 })).toBe(
      "The folder is on the branch somewhere-else, which stays. The branch check-the-queue-b5rc0def, which purlis cut, is removed only if it is already merged.",
    );
    expect(lostSaid(moved)).toBeUndefined();
  });

  it("says of a dispatch that never started what it was, who asked and how it stands (#1456)", () => {
    const row: NotStartedRow = {
      state: "held",
      mode: "handoff",
      persona: null,
      task: null,
      asker: null,
      by_person: false,
      at: null,
    };
    expect(notStartedSaid(row)).toBe(
      "A handoff for the asking chat's own persona, asked by a chat that has closed: waiting for your answer on its Notice.",
    );
    expect(
      notStartedSaid({ ...row, state: "kept-blocked", by_person: true, asker: "steward 3" }),
    ).toBe(
      "A handoff for the asking chat's own persona, asked by you, from steward 3: kept blocked by you. Nothing was started.",
    );
  });

  it("says of a dispatch waiting on memory that it starts by itself, and when it gave up (#1467)", () => {
    const row: NotStartedRow = {
      state: "waiting-on-memory",
      mode: "task",
      persona: "devops",
      task: "rotate the keys",
      asker: "steward 3",
      by_person: false,
      at: "2026-10-09T08:30:00Z",
    };
    expect(waitsOnMemory(row)).toBe(true);
    expect(notStartedSaid(row)).toBe(
      "The task rotate the keys for devops, asked by steward 3: waiting for this machine to free memory (2026-10-09 08:30 UTC). It starts by itself once memory frees, or after 10 minutes starts nothing.",
    );
    const gaveUp = { ...row, state: "gave-up-on-memory" };
    expect(waitsOnMemory(gaveUp)).toBe(false);
    expect(notStartedSaid(gaveUp)).toBe(
      "The task rotate the keys for devops, asked by steward 3: this machine was still short on memory after 10 minutes (2026-10-09 08:30 UTC). Nothing was started.",
    );
    expect(waitsOnMemory({ ...row, state: "held" })).toBe(false);
  });

  it("is one view per project, which no extension's view of the same name is", () => {
    expect(isDispatches({ from: null, view: "dispatches", key: "" })).toBe(true);
    expect(isDispatches({ from: "someone", view: "dispatches", key: "" })).toBe(false);
    expect(isDispatches({ from: null, view: "session", key: "" })).toBe(false);
  });
});
