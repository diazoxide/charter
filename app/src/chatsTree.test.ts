import { describe, expect, it } from "vitest";
import type { OpenChat } from "./bindings";
import {
  below,
  belowCount,
  cameFromSaid,
  chatsTree,
  handedOff,
  handedOffSaid,
  listedChat,
  needing,
  ownBranch,
  parentsIn,
  startedBy,
  tasksOf,
  unfolded,
  type ListedChat,
} from "./chatsTree";

/** A chat started by `parent`, where one started it. */
function listed(session: number, parent: number | null = null): ListedChat {
  return {
    session,
    name: `chat ${session}`,
    persona: null,
    workspace: "alpha",
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
  };
}

/** The tree as `level:session`, top to bottom. */
const drawn = (chats: ListedChat[]) => chatsTree(chats).map((row) => `${row.level}:${row.session}`);

describe("the project's chats as a tree", () => {
  it("draws each chat under the chat that started it, depth first", () => {
    expect(drawn([listed(1), listed(2, 1), listed(3), listed(4, 2), listed(5, 1)])).toEqual([
      "1:1",
      "2:2",
      "3:4",
      "2:5",
      "1:3",
    ]);
  });

  it("says where each row is among its siblings", () => {
    const rows = chatsTree([listed(1), listed(2, 1), listed(3, 1), listed(4)]);
    expect(rows.map((row) => `${row.session} ${row.posinset}/${row.setsize}`)).toEqual([
      "1 1/2",
      "2 1/2",
      "3 2/2",
      "4 2/2",
    ]);
  });

  it("leaves a chat whose parent has closed at the top, marked, with its own children under it", () => {
    const rows = chatsTree([listed(1), listed(8, 7), listed(9, 8)]);
    expect(rows.map((row) => `${row.level}:${row.session}:${row.orphaned}`)).toEqual([
      "1:1:false",
      "1:8:true",
      "2:9:false",
    ]);
  });

  it("marks no chat of a loop as having a closed parent: its parent is open", () => {
    const rows = chatsTree([listed(1), listed(2, 3), listed(3, 2)]);
    expect(rows.every((row) => !row.orphaned)).toBe(true);
  });

  it("says a closed asker closed, and one waiting to start is not open", () => {
    expect(cameFromSaid("steward 7", true)).toBe("asked by steward 7 (closed)");
    expect(cameFromSaid("steward 7", true, true)).toBe("asked by steward 7 (not open)");
    expect(cameFromSaid("steward 7", false)).toBe("from steward 7");
  });

  it("draws every chat once when the lineage loops back on itself", () => {
    // A number dealt again could make two chats each other's parent. Neither may vanish.
    expect(drawn([listed(1), listed(2, 3), listed(3, 2), listed(4, 4)]).sort()).toEqual([
      "1:1",
      "1:2",
      "1:4",
      "2:3",
    ]);
  });

  it("says the branch of its own a task works on, from the folder purlis cut for it", () => {
    // #1453. Only a task's: a chat the person started in a branch's folder is not a dispatch's.
    const chat = (cwd: string | null, task: boolean | null) =>
      ({
        cwd,
        from:
          task === null
            ? null
            : { name: "steward 1", workspace: "alpha", chat: 1, task, tab: !task },
      }) as Parameters<typeof ownBranch>[0];
    const folder = "/p/workspaces/alpha/.worktrees/api/check-the-queue-b5rc0def";
    expect(ownBranch(chat(folder, true))).toBe("check-the-queue-b5rc0def");
    expect(ownBranch(chat(`${folder}/src/deep`, true))).toBe("check-the-queue-b5rc0def");
    // A task in the asking chat's folder, or in another workspace's, has none.
    expect(ownBranch(chat("/p/workspaces/alpha/api", true))).toBeNull();
    expect(ownBranch(chat("/p/workspaces/beta", true))).toBeNull();
    expect(ownBranch(chat(null, true))).toBeNull();
    // A handoff's chat and one the person opened are not tasks.
    expect(ownBranch(chat(folder, false))).toBeNull();
    expect(ownBranch(chat(folder, null))).toBeNull();
  });

  it("lists what each chat started, by the asking chat's number, open parents only", () => {
    const by = startedBy([listed(1), listed(2, 1), listed(3, 1), listed(8, 7)]);
    expect([...by.keys()]).toEqual([1]);
    expect(by.get(1)?.map((chat) => chat.session)).toEqual([2, 3]);
  });
});

/** 1 started 2 and 5; 2 started 3; 3 started 4. 6 is on its own. */
const deep = () => [listed(1), listed(2, 1), listed(3, 2), listed(4, 3), listed(5, 1), listed(6)];

describe("the needs-you mark rolling up the tree (#1448)", () => {
  /** Each marked row as `row>chat it leads to`. */
  const leads = (needsYou: number[]) =>
    [...needing(chatsTree(deep()), needsYou)].map(([row, to]) => `${row}>${to}`);

  it("marks the chat that needs you and every chat above it, each leading to that chat", () => {
    expect(leads([4])).toEqual(["1>4", "2>4", "3>4", "4>4"]);
  });

  it("marks nothing beside or below the chat that needs you", () => {
    expect(leads([2])).toEqual(["1>2", "2>2"]);
    expect(leads([6])).toEqual(["6>6"]);
    expect(leads([])).toEqual([]);
  });

  it("leads a chat that needs you itself to itself, whatever is below it", () => {
    expect(leads([4, 2])).toEqual(["1>4", "2>2", "3>4", "4>4"]);
  });

  it("leads a row over several to the one that has needed you longest", () => {
    // 5 asked before 4 did: the queue is oldest first.
    expect(leads([5, 4])).toEqual(["1>5", "2>4", "3>4", "4>4", "5>5"]);
  });
});

describe("folding a row of the tree (#1448)", () => {
  const shown = (folded: number[]) =>
    unfolded(chatsTree(deep()), new Set(folded)).map((row) => row.session);

  it("says which rows have rows under them", () => {
    expect([...parentsIn(chatsTree(deep()))]).toEqual([1, 2, 3]);
  });

  it("draws every row while nothing is folded", () => {
    expect(shown([])).toEqual([1, 2, 3, 4, 5, 6]);
  });

  it("leaves out everything under a folded row, at any depth, and nothing else", () => {
    expect(shown([2])).toEqual([1, 2, 5, 6]);
    expect(shown([1])).toEqual([1, 6]);
    expect(shown([3, 1])).toEqual([1, 6]);
  });

  it("still marks a folded row for a chat it hides", () => {
    const rows = chatsTree(deep());
    const drawnRows = unfolded(rows, new Set([2]));
    const marks = needing(rows, [4]);
    expect(drawnRows.filter((row) => marks.has(row.session)).map((row) => row.session)).toEqual([
      1, 2,
    ]);
    expect(marks.get(2)).toBe(4);
  });
});

describe("what is below a chat (#1448)", () => {
  it("counts every running chat nested under it", () => {
    expect(belowCount(deep(), 1)).toBe(4);
    expect(belowCount(deep(), 2)).toBe(2);
    expect(belowCount(deep(), 4)).toBe(0);
    expect(belowCount(deep(), 9)).toBe(0);
  });
});

/** A chat the work of `parent` was handed off to. */
const handoff = (session: number, parent: number, name = `chat ${session}`): ListedChat => ({
  ...listed(session, parent),
  name,
  mode: "handoff",
  tab: true,
});

describe("a handoff in the tree (#1492, V100-69)", () => {
  /** 1 asked 2 for a task and handed off to 3, which asked 4 for a task. */
  const moved = () => [listed(1), listed(2, 1), handoff(3, 1), listed(4, 3)];

  it("stands at the top, never under the chat it came from, with its own tasks under it", () => {
    expect(drawn(moved())).toEqual(["1:1", "2:2", "1:3", "2:4"]);
  });

  it("is an orphan only once the chat it came from has closed", () => {
    const open = chatsTree(moved()).find((row) => row.session === 3);
    expect(open?.orphaned).toBe(false);
    const closed = chatsTree([handoff(3, 1), listed(4, 3)]).find((row) => row.session === 3);
    expect(closed?.orphaned).toBe(true);
  });

  it("is not one of the tasks of the chat it came from, nor below it", () => {
    expect(
      tasksOf(moved())
        .get(1)
        ?.map((chat) => chat.session),
    ).toEqual([2]);
    expect(
      tasksOf(moved())
        .get(3)
        ?.map((chat) => chat.session),
    ).toEqual([4]);
    expect(below(moved(), 1)).toEqual([2]);
    expect(belowCount(moved(), 1)).toBe(1);
    expect(parentsIn(chatsTree([listed(1), handoff(3, 1)])).has(1)).toBe(false);
  });

  it("does not put the hand on the row of the chat it came from when it needs you", () => {
    const marks = needing(chatsTree(moved()), [4]);
    expect([...marks.keys()]).toEqual([3, 4]);
  });

  it("is where the work of the chat it came from went, the newest first, open chats only", () => {
    const went = handedOff([
      listed(1),
      listed(2, 1),
      handoff(3, 1, "drop commons"),
      handoff(5, 1, "release notes"),
      handoff(6, 9, "from a closed chat"),
    ]);
    expect(went.get(1)?.map((chat) => chat.name)).toEqual(["release notes", "drop commons"]);
    // A task is not a handoff, and the chat that handed off is said nothing of itself.
    expect(went.has(2)).toBe(false);
    expect(went.has(3)).toBe(false);
  });

  it("says the newest chat by name, and how many more there are", () => {
    expect(handedOffSaid("release notes", 0)).toBe("handed off to release notes");
    expect(handedOffSaid("release notes", 2)).toBe("handed off to release notes and 2 more");
  });
});

describe("a task the person asked for (#1492, V100-70)", () => {
  const from = {
    chat: 1,
    name: "steward 1",
    workspace: "alpha",
    task: true,
    tab: false,
    reported: false,
    unreported: false,
  };
  const chat = (more: Partial<NonNullable<OpenChat["from"]>>) =>
    listedChat(
      {
        session: 4,
        name: "4",
        cwd: null,
        harness: "claude",
        in_front: false,
        resumed: null,
        fresh: null,
        guessed: null,
        profile: null,
        persona: "devops",
        unreported: null,
        card: null,
        pinned: false,
        label: null,
        from: { ...from, ...more },
      },
      "alpha",
      "devops 4",
      false,
    );

  it("is a task of the session it was asked from, marked as the person's", () => {
    const theirs = chat({ by_person: true });
    expect([theirs.mode, theirs.parent, theirs.byYou]).toEqual(["task", 1, true]);
    expect(drawn([listed(1), theirs])).toEqual(["1:1", "2:4"]);
  });

  it("is not marked where the chat dispatched it, and a handoff never is", () => {
    expect(chat({}).byYou).toBe(false);
    expect(chat({ task: false, tab: true, by_person: true }).byYou).toBe(false);
  });
});
