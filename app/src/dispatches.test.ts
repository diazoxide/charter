import { describe, expect, it } from "vitest";
import type { DispatchRow, WorktreeLoss } from "./bindings";
import {
  askersOf,
  branchSaid,
  counted,
  discardSays,
  EVERY_DISPATCH,
  isDispatches,
  losesNothing,
  lostSaid,
  NO_PERSONA,
  personasOf,
  saidAt,
  shownDispatches,
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
    expect(ids(shownDispatches(ROWS, { persona: "devops", asker: "" }))).toEqual(["c", "a"]);
    const planner = "id of planner 2";
    expect(ids(shownDispatches(ROWS, { persona: "", asker: planner }))).toEqual(["a", "z"]);
    expect(ids(shownDispatches(ROWS, { persona: "devops", asker: planner }))).toEqual(["a"]);
    expect(ids(shownDispatches(ROWS, { persona: "qa", asker: planner }))).toEqual([]);
  });

  it("finds the dispatches that went to no persona, which no persona's name could mean", () => {
    expect(ids(shownDispatches(ROWS, { persona: NO_PERSONA, asker: "" }))).toEqual(["z"]);
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
    expect(ids(shownDispatches(rows, { persona: "", asker: "01OLD" }))).toEqual(["old"]);
    // A name is not a key: nothing is found by it.
    expect(ids(shownDispatches(rows, { persona: "", asker: "steward 3" }))).toEqual([]);
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
    unmerged: 12,
    lost: [],
  };

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

  it("is one view per project, which no extension's view of the same name is", () => {
    expect(isDispatches({ from: null, view: "dispatches", key: "" })).toBe(true);
    expect(isDispatches({ from: "someone", view: "dispatches", key: "" })).toBe(false);
    expect(isDispatches({ from: null, view: "session", key: "" })).toBe(false);
  });
});
