import { describe, expect, it } from "vitest";
import { chatsTree, type ListedChat } from "./chatsTree";
import { inScope } from "./chatsScope";

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
