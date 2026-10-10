import { describe, expect, it } from "vitest";
import { chatsTree, type ListedChat } from "./chatsTree";
import { inScope, inTab, keepScope, keptScope } from "./chatsScope";

/** A chat working in `workspace`, started by `parent` as `mode` where one started it. */
function listed(
  session: number,
  workspace: string,
  parent: number | null = null,
  mode: "task" | "handoff" | null = parent === null ? null : "task",
): ListedChat {
  return {
    session,
    name: `chat ${session}`,
    persona: null,
    workspace,
    shell: false,
    parent,
    mode,
    from: parent === null ? null : `chat ${parent}`,
    tab: mode !== "task",
    branch: null,
    report: null,
    outcome: null,
    asking: null,
    harness: null,
  };
}

/** The rows as `level:session posinset/setsize`. */
const shape = (rows: ReturnType<typeof inScope>) =>
  rows.map((row) => `${row.level}:${row.session} ${row.posinset}/${row.setsize}`);

describe("the trees a workspace lists (#1655)", () => {
  const rows = chatsTree([
    listed(1, "alpha"),
    // Asked for by alpha's chat, working in beta: alpha's tree.
    listed(2, "beta", 1),
    listed(3, "beta"),
    // Asked for by beta's chat, working in alpha: beta's tree.
    listed(4, "alpha", 3),
    // Handed off from alpha to beta: the work moved, so beta's.
    listed(5, "beta", 1, "handoff"),
    listed(6, "plane root"),
  ]);

  it("keeps each tree whose top row works there, with every row below it, counted again", () => {
    expect(shape(inScope(rows, "alpha"))).toEqual(["1:1 1/1", "2:2 1/1"]);
    expect(shape(inScope(rows, "beta"))).toEqual(["1:3 1/2", "2:4 1/1", "1:5 2/2"]);
  });

  it("lists a chat started at the plane root in the root's view only", () => {
    expect(shape(inScope(rows, "plane root"))).toEqual(["1:6 1/1"]);
  });

  it("gives back the same rows where it leaves nothing out", () => {
    const alone = chatsTree([listed(1, "alpha"), listed(2, "beta", 1)]);
    expect(inScope(alone, "alpha")).toBe(alone);
  });
});

describe("the chats a tab lists (#1679)", () => {
  const rows = chatsTree([
    listed(1, "alpha"),
    listed(2, "alpha", 1),
    // A task of 2's, in a tab of its own: that tab's, with what it asked for.
    listed(3, "beta", 2),
    listed(4, "beta", 3),
    listed(5, "alpha", 1),
    listed(6, "beta"),
  ]);

  it("keeps the tab's chats in the tree's order, nested as they are", () => {
    expect(shape(inTab(rows, new Set([1, 2, 3, 5])))).toEqual([
      "1:1 1/1",
      "2:2 1/2",
      "3:3 1/1",
      "2:5 2/2",
    ]);
  });

  it("stands a chat whose asker is not the tab's at the top, with what is below it", () => {
    expect(shape(inTab(rows, new Set([3, 4, 6])))).toEqual(["1:3 1/2", "2:4 1/1", "1:6 2/2"]);
  });

  it("lists nothing for a tab that holds no chat", () => {
    expect(inTab(rows, new Set())).toEqual([]);
  });

  it("gives back the same rows where it leaves nothing out", () => {
    expect(inTab(rows, new Set([1, 2, 3, 4, 5, 6]))).toBe(rows);
  });
});

describe("the scope kept for each project (#1679)", () => {
  it("is the workspace's until one is picked, and each project's own", () => {
    expect(keptScope("/p/one")).toBe("workspace");
    keepScope("/p/one", "tab");
    keepScope("/p/two", "all");
    expect(keptScope("/p/one")).toBe("tab");
    expect(keptScope("/p/two")).toBe("all");
    keepScope("/p/one", "workspace");
    expect(keptScope("/p/one")).toBe("workspace");
  });

  it("is the workspace's where what is kept is not a scope", () => {
    globalThis.localStorage.setItem("purlis.chats.scope:/p/one", "everything");
    expect(keptScope("/p/one")).toBe("workspace");
  });
});
