import { afterEach, describe, expect, it } from "vitest";
import { chatsTree, type ListedChat } from "./chatsTree";
import { inScope, inTab, keepScope, keptScope, settleScope, tabSessions } from "./chatsScope";
import { keptFacet } from "./projectViews";
import { GLOBAL, usingTheDefaultLayout } from "./windowprefs";

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

describe("the chats This tab holds (#1696)", () => {
  const tab = (...sessions: number[]) => chatsTree(sessions.map((one) => listed(one, "alpha")));
  const every = chatsTree([
    listed(1, "alpha"),
    listed(2, "alpha", 1),
    listed(3, "alpha", 1),
    listed(4, "alpha"),
  ]);

  it("are the tab's own chats", () => {
    expect([...tabSessions(tab(1, 2, 3), undefined, every)]).toEqual([1, 2, 3]);
  });

  it("keep a task that ended while the list keeps its row, until its finished row is read", () => {
    const before = tabSessions(tab(1, 2, 3), undefined, every);
    // Task 3 ended: it is in no tab now, and the list still keeps its row.
    const now = tabSessions(tab(1, 2), before, every);
    expect([...now].sort()).toEqual([1, 2, 3]);
    // Its finished row was read: the list lets the row go, and so does the tab.
    const read = every.filter((row) => row.session !== 3);
    expect([...tabSessions(tab(1, 2), now, read)].sort()).toEqual([1, 2]);
  });

  it("keep nothing for a chat that is not a task, nor for another tab's", () => {
    const before = tabSessions(tab(1, 4), undefined, every);
    expect([...tabSessions(tab(1), before, every)]).toEqual([1]);
    expect([...tabSessions(tab(4), undefined, every)]).toEqual([4]);
  });

  it("answer what they answered before where nothing changed, so nothing is drawn again", () => {
    const before = tabSessions(tab(1, 2, 3), undefined, every);
    expect(tabSessions(tab(1, 2, 3), before, every)).toBe(before);
    const kept = tabSessions(tab(1, 2), before, every);
    expect(tabSessions(tab(1, 2), kept, every)).toBe(kept);
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

describe("the scope in layout.json v2 (B-11, #1696)", () => {
  const put = (document: unknown) => {
    (globalThis as Record<string, unknown>)[GLOBAL] = {
      layout: { path: "/cfg/layout.json", found: true, document, trouble: null },
      theme: { path: "", found: false, document: null, trouble: null },
    };
  };
  afterEach(() => Reflect.deleteProperty(globalThis, GLOBAL));

  it("is kept in the project's entry, and the default is kept as nothing", () => {
    keepScope("/p/one", "all");
    expect(keptFacet("/p/one", "chats")).toEqual({ scope: "all" });

    keepScope("/p/one", "workspace");
    expect(keptFacet("/p/one", "chats")).toBeUndefined();
  });

  it("is read from the project's entry in the file, and a value that is not a scope is the default", () => {
    put({
      version: 2,
      regions: [],
      projects: { "/p/one": { chats: { scope: "tab" } }, "/p/two": { chats: { scope: "x" } } },
    });

    expect(keptScope("/p/one")).toBe("tab");
    expect(keptScope("/p/two")).toBe("workspace");
  });

  it("moves what web storage kept into the file once, and the key goes", () => {
    globalThis.localStorage.setItem("purlis.chats.scope:/p/one", "all");
    expect(keptScope("/p/one")).toBe("all");

    settleScope("/p/one");

    expect(keptFacet("/p/one", "chats")).toEqual({ scope: "all" });
    expect(globalThis.localStorage.getItem("purlis.chats.scope:/p/one")).toBeNull();
    expect(keptScope("/p/one")).toBe("all");
  });

  it("never lets web storage's old pick over the file's, and still lets the key go", () => {
    put({ version: 2, regions: [], projects: { "/p/one": { chats: { scope: "tab" } } } });
    globalThis.localStorage.setItem("purlis.chats.scope:/p/one", "all");

    expect(keptScope("/p/one")).toBe("tab");
    settleScope("/p/one");

    expect(keptScope("/p/one")).toBe("tab");
    expect(globalThis.localStorage.getItem("purlis.chats.scope:/p/one")).toBeNull();
  });

  it("is the workspace's again once the default layout is used", () => {
    put({ version: 2, regions: [], projects: { "/p/one": { chats: { scope: "tab" } } } });

    usingTheDefaultLayout();

    expect(keptScope("/p/one")).toBe("workspace");
  });
});
