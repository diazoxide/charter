import { describe, expect, it } from "vitest";
import type { ListedChat } from "./chatsTree";
import {
  askedByOf,
  chatsOfPane,
  chatsOfTab,
  chatsOfTabs,
  crumbsOf,
  hiddenNeeding,
} from "./tabChats";
import { homeOf, noTabs, openTab, panesOf, selectTab, switchTabTo, type Tabs } from "./tabs";

/** A listed chat: `parent` is the chat that started it, as a task unless said otherwise. */
function listed(
  session: number,
  name: string,
  parent: number | null = null,
  more: Partial<ListedChat> = {},
): ListedChat {
  return {
    session,
    name,
    persona: "steward",
    workspace: "alpha",
    shell: false,
    parent,
    mode: parent === null ? null : "task",
    from: null,
    tab: parent === null,
    branch: null,
    report: parent === null ? null : "owed",
    outcome: null,
    asking: null,
    harness: "Claude Code",
    ...more,
  };
}

/**
 * steward 1 dispatched talk (4) and sweep (5); talk dispatched probe (6), which works in beta.
 * steward 2 dispatched notes (7). drop commons (3) is a handoff from steward 1, in its own tab.
 */
const chats = [
  listed(1, "steward 1"),
  listed(2, "steward 2"),
  listed(3, "drop commons", 1, { mode: "handoff", report: null }),
  listed(4, "talk", 1),
  listed(5, "sweep", 1),
  listed(6, "probe", 4, { workspace: "beta" }),
  listed(7, "notes", 2),
  listed(8, "under the handoff", 3),
];

/** A tab each for 1, 2 and the handoff 3; the last opened is in front. */
function tabs(): Tabs {
  return openTab(openTab(openTab(noTabs(), 1), 2), 3);
}

function tabOf(all: Tabs, session: number): number {
  const found = all.order.find((id) => panesOf(all, id).some((one) => one.session === session));
  if (found === undefined) throw new Error(`no tab is session ${session}'s`);
  return found;
}

const askedBy = askedByOf(chats);

describe("the chats of a tab, as a tree", () => {
  it("lists every tab's chats in one walk exactly as it lists each tab's", () => {
    const all = switchTabTo(tabs(), 6, askedBy);
    const every = chatsOfTabs(all, chats);

    expect([...every.keys()]).toEqual(all.order);
    for (const id of all.order) expect(every.get(id)).toEqual(chatsOfTab(all, id, chats));
    expect(every.get(tabOf(all, 1))?.map((row) => `${row.level} ${row.name}`)).toEqual([
      "1 steward 1",
      "2 talk",
      "3 probe",
      "2 sweep",
    ]);
  });

  it("is the session's own chat and every task below it, nested by who asked", () => {
    const all = tabs();
    expect(chatsOfTab(all, tabOf(all, 1), chats).map((row) => `${row.level} ${row.name}`)).toEqual([
      "1 steward 1",
      "2 talk",
      "3 probe",
      "2 sweep",
    ]);
  });

  it("leaves out a chat with a tab of its own, and everything below that chat", () => {
    const all = tabs();
    const names = chatsOfTab(all, tabOf(all, 1), chats).map((row) => row.name);
    expect(names).not.toContain("drop commons");
    expect(names).not.toContain("under the handoff");
    expect(chatsOfTab(all, tabOf(all, 3), chats).map((row) => `${row.level} ${row.name}`)).toEqual([
      "1 drop commons",
      "2 under the handoff",
    ]);
  });

  it("is the session alone for a session with no tasks, and nothing for a tab that is not there", () => {
    const all = openTab(noTabs(), 9);
    expect(chatsOfTab(all, tabOf(all, 9), [listed(9, "steward 9")]).map((row) => row.name)).toEqual(
      ["steward 9"],
    );
    expect(chatsOfTab(all, 99, chats)).toEqual([]);
  });

  it("says where each row stands among the rows beside it", () => {
    const all = tabs();
    const rows = chatsOfTab(all, tabOf(all, 1), chats);
    const sweep = rows.find((row) => row.name === "sweep");
    expect(sweep).toMatchObject({ level: 2, posinset: 2, setsize: 2, orphaned: false });
  });
});

describe("the path to the chat a pane shows", () => {
  it("is nothing while the pane shows its session's own chat", () => {
    expect(crumbsOf(tabs(), tabOf(tabs(), 1), chats)).toEqual({});
  });

  it("is the session, then the task, for a task", () => {
    const all = switchTabTo(tabs(), 5, askedBy);
    const crumbs = Object.values(crumbsOf(all, tabOf(all, 1), chats));
    expect(crumbs).toHaveLength(1);
    expect(crumbs[0].path.map((chat) => chat.name)).toEqual(["steward 1", "sweep"]);
    // It works where its session does, so nothing says where.
    expect(crumbs[0].elsewhere).toBeNull();
  });

  it("is the whole path for a task of a task, and says the workspace it works in", () => {
    const all = switchTabTo(tabs(), 6, askedBy);
    const crumbs = crumbsOf(all, tabOf(all, 1), chats);
    const [pane] = panesOf(all, tabOf(all, 1));
    expect(crumbs[pane.pane].path.map((chat) => chat.name)).toEqual(["steward 1", "talk", "probe"]);
    expect(crumbs[pane.pane].elsewhere).toBe("beta");
  });

  it("is nothing for a shown chat the list does not hold yet", () => {
    const all = switchTabTo(tabs(), 6, askedBy);
    expect(
      crumbsOf(
        all,
        tabOf(all, 1),
        chats.filter((chat) => chat.session !== 6),
      ),
    ).toEqual({});
  });
});

describe("the chats of a tab that are waiting and are not on screen", () => {
  it("is each task of the tab in the list that the tab is not showing, longest waiting first", () => {
    const all = selectTab(tabs(), tabOf(tabs(), 1));
    expect(hiddenNeeding(all, tabOf(all, 1), askedBy, [6, 7, 4])).toEqual([6, 4]);
    expect(hiddenNeeding(all, tabOf(all, 2), askedBy, [6, 7, 4])).toEqual([7]);
  });

  it("leaves out the task on screen in the tab in front", () => {
    const all = switchTabTo(tabs(), 4, askedBy);
    expect(hiddenNeeding(all, tabOf(all, 1), askedBy, [4, 6])).toEqual([6]);
  });

  it("counts the task a tab was left showing, once the tab is not in front", () => {
    const left = selectTab(switchTabTo(tabs(), 4, askedBy), tabOf(tabs(), 2));
    expect(hiddenNeeding(left, tabOf(left, 1), askedBy, [4])).toEqual([4]);
  });

  it("counts the session's own chat while its pane shows a task, and not while it shows it", () => {
    const shown = switchTabTo(tabs(), 4, askedBy);
    expect(hiddenNeeding(shown, tabOf(shown, 1), askedBy, [1])).toEqual([1]);
    // Its own chat on its own pane: the tab's state mark is that chat's, in front or behind.
    expect(hiddenNeeding(tabs(), tabOf(tabs(), 1), askedBy, [1])).toEqual([]);
    expect(hiddenNeeding(tabs(), tabOf(tabs(), 2), askedBy, [2])).toEqual([]);
  });

  it("is nothing for a quiet list", () => {
    expect(hiddenNeeding(tabs(), tabOf(tabs(), 1), askedBy, [])).toEqual([]);
  });
});

describe("a handoff", () => {
  it("is never at home in the tab of the chat it came from, with or without a tab here", () => {
    // Only 1 and 2 have tabs in this window: the handoff 3 has none.
    const two = openTab(openTab(noTabs(), 1), 2);
    expect(homeOf(two, 3, askedBy)).toBeUndefined();
    expect(homeOf(two, 8, askedBy)).toBeUndefined();
    expect(chatsOfTab(two, tabOf(two, 1), chats).map((row) => row.name)).toEqual([
      "steward 1",
      "talk",
      "probe",
      "sweep",
    ]);
  });
});

describe("the chats of one pane", () => {
  it("is the pane's own chat first, then the tasks at home in it", () => {
    const all = tabs();
    const [pane] = panesOf(all, tabOf(all, 2));
    expect(chatsOfPane(all, tabOf(all, 2), pane.pane, chats).map((row) => row.name)).toEqual([
      "steward 2",
      "notes",
    ]);
  });
});
