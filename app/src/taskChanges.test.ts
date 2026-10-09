import { describe, expect, it } from "vitest";
import type { BranchMerge, OwnBranch } from "./bindings";
import { leftSaid, mergeBlocked, mergeSays, taskChangesOf, taskChangesView } from "./taskChanges";

const MERGE: BranchMerge = {
  task: "fix the queue",
  repo: "api",
  branch: "fix-the-queue-0000aaaa",
  into: "main",
  tip: "a".repeat(40),
  ahead: 1,
  behind: 0,
  uncommitted: [],
};

describe("a task's Changes tab", () => {
  it("is named by the dispatch's id and nothing else", () => {
    const view = taskChangesView("01K6TASK");

    expect(view).toEqual({ from: null, view: "task-changes", key: "01K6TASK" });
    expect(taskChangesOf(view)).toBe("01K6TASK");
    // Another kind of view, an extension's, or one with no id is no task's.
    expect(taskChangesOf({ from: null, view: "changes", key: "alpha" })).toBeUndefined();
    expect(taskChangesOf({ from: "ext", view: "task-changes", key: "01K6TASK" })).toBeUndefined();
    expect(taskChangesOf({ from: null, view: "task-changes", key: "" })).toBeUndefined();
  });

  it("says what a merge lands, and that it only fast-forwards", () => {
    expect(mergeSays(MERGE)).toBe(
      "This lands 1 commit of fix-the-queue-0000aaaa, which purlis cut for fix the queue, in main in api. purlis only fast-forwards: it writes no merge commit and never forces one.",
    );
    expect(mergeBlocked(MERGE)).toBeUndefined();
  });

  it("says why a merge would change nothing before anything is asked", () => {
    expect(mergeBlocked({ ...MERGE, behind: 2 })).toBe(
      "main has 2 commits the task's branch does not have, so the branch no longer fast-forwards. Merge main into fix-the-queue-0000aaaa where its conflicts belong, then merge again.",
    );
    expect(mergeBlocked({ ...MERGE, uncommitted: ["?? scratch.txt"] })).toMatch(
      /^Its folder holds changes that are not committed/,
    );
    expect(mergeBlocked({ ...MERGE, ahead: 0 })).toBe(
      "fix-the-queue-0000aaaa has nothing to land in main.",
    );
    expect(mergeBlocked({ ...MERGE, into: null })).toMatch(/^purlis has no record of the branch/);
  });
});

describe("what is left of a branch whose folder is gone (#1472)", () => {
  const LEFT = {
    tip: "a".repeat(40),
    base: "main",
    landed: "squashed",
    ahead: 3,
    checked_out: false,
    deletable: false,
  };
  const OWN: OwnBranch = {
    repo: "api",
    branch: "fix-the-queue-0000aaaa",
    standing: "merged-branch-kept",
    acts: false,
    left: LEFT,
  };

  it("says a squashed branch is in the branch it was cut from, as its row does", () => {
    expect(leftSaid(OWN)).toBe(
      "The branch fix-the-queue-0000aaaa is still in api. Its changes are in main, squashed or rebased in, but its own 3 commits are not. git does not find it merged into the branch api is on, so it stays: merge it, or delete it with git, yourself.",
    );
  });

  it("never offers to delete a branch the repo is on", () => {
    const on = { ...OWN, left: { ...LEFT, landed: "merged", ahead: 0, checked_out: true } };
    expect(leftSaid(on)).toBe(
      "The branch fix-the-queue-0000aaaa is still in api, and main holds every commit of it. api is on it now, so it is not deleted here: git deletes no branch a repo is on.",
    );
  });

  it("says nothing where the folder is there", () => {
    expect(leftSaid({ ...OWN, left: null })).toBeUndefined();
  });
});
