import { describe, expect, it } from "vitest";
import type { ListedChat } from "./chatsTree";
import { chatsOfPane, chatsOfTab, chatsOfTabs, crumbsOf, placedCrumbsOf } from "./tabChats";
import {
  chatOf,
  closeTab,
  focusedChat,
  homeOf,
  moveToOwnTab,
  noTabs,
  openBeside,
  openTab,
  panesOf,
  placeBeside,
  placedOf,
  selectTab,
  sendBack,
  sessionOf,
  shownIn,
  switchTabTo,
  tasksPlacedBelow,
  visibleSessions,
  type AskedBy,
  type FiledIn,
  type Tabs,
} from "./tabs";

/**
 * **A task with a pane of its own** (#1489): in a tab of its own, which the person asked for,
 * or beside the session that asked for it, inside that session's tab. The layout already says
 * it (a chat with a pane is its own home, `tabs.homeOf`); these are the ways in and out, and
 * what the session's tab still says of a task that has left its pane.
 */

const oneWorkspace: FiledIn = () => "alpha";

const lineage =
  (asked: Record<number, number>): AskedBy =>
  (session) =>
    asked[session];

/** Chat 1 dispatched 4 and 5; 4 dispatched 6; chat 2 dispatched 7; chat 9, closed, 12. */
const askedBy = lineage({ 4: 1, 5: 1, 6: 4, 7: 2, 12: 9 });

/** Two sessions, each in a tab of its own: 1 and 2. The first is in front. */
function twoSessions(): Tabs {
  return selectTab(openTab(openTab(noTabs(), 1, "1", "steward"), 2, "2", "steward"), 1);
}

function tabOf(tabs: Tabs, session: number): number {
  const found = tabs.order.find((id) => panesOf(tabs, id).some((one) => one.session === session));
  if (found === undefined) throw new Error(`no tab holds session ${session}`);
  return found;
}

const sessionsOf = (tabs: Tabs) => tabs.order.map((id) => panesOf(tabs, id).map((p) => p.session));

describe("moving a task to a tab of its own", () => {
  it("gives it a tab at the strip's end, in front, and leaves its session's tab as it was", () => {
    const tabs = moveToOwnTab(twoSessions(), 4, "talk", "devops", null);

    expect(sessionsOf(tabs)).toEqual([[1], [2], [4]]);
    expect(tabs.inFront).toBe(tabOf(tabs, 4));
    expect(chatOf(tabs, tabOf(tabs, 4))).toBe(4);
    expect(placedOf(tabs, 4, askedBy)).toBe("tab");
    expect(homeOf(tabs, 4, askedBy)?.own).toBe(4);
    // Its session is still where it goes back to.
    expect(sessionOf(tabs, 4, askedBy)).toMatchObject({ tab: tabOf(tabs, 1), own: 1 });
  });

  it("takes it out of the pane that was showing it, which shows its own chat again", () => {
    const shown = switchTabTo(twoSessions(), 4, askedBy);
    expect(visibleSessions(shown)).toEqual([4]);

    const tabs = moveToOwnTab(shown, 4, "talk", "devops", null);

    // One chat, one place: the session's tab does not go on showing it as "moved".
    expect(shownIn(tabs, tabOf(tabs, 1))).toEqual([
      expect.objectContaining({ own: 1, session: 1 }),
    ]);
    expect(tabs.byId[tabOf(tabs, 1)].shows).toBeUndefined();
  });

  it("carries its own tasks with it: they are at home in its tab", () => {
    const tabs = moveToOwnTab(twoSessions(), 4, "talk", "devops", null);
    expect(homeOf(tabs, 6, askedBy)).toMatchObject({ tab: tabOf(tabs, 4), own: 4 });
    expect(placedOf(tabs, 6, askedBy)).toBeUndefined();
  });

  it("takes a task of its own out of the pane that was showing that, too", () => {
    // The session's pane shows 6, a task of 4. Moved, 4 is 6's home: the pane it leaves could
    // not draw 6, and would go on saying to the core that 6 is on screen.
    const shown = switchTabTo(twoSessions(), 6, askedBy);
    expect(visibleSessions(shown)).toEqual([6]);

    const own = moveToOwnTab(shown, 4, "talk", "devops", null, askedBy);
    expect(own.byId[tabOf(own, 1)].shows).toBeUndefined();
    const beside = openBeside(shown, 4, askedBy, "talk");
    expect(visibleSessions(beside)).toEqual([1, 4]);
    expect(beside.byId[tabOf(beside, 1)].shows).toBeUndefined();
    // A task that is not below the one moved stays where it is shown.
    const other = moveToOwnTab(
      switchTabTo(twoSessions(), 5, askedBy),
      4,
      "talk",
      "devops",
      null,
      askedBy,
    );
    expect(shownIn(other, tabOf(other, 1))[0].session).toBe(5);
  });

  it("brings its tab forward when it already has one, and adds none", () => {
    const once = selectTab(moveToOwnTab(twoSessions(), 4, "talk", "devops", null), 1);
    const twice = moveToOwnTab(once, 4, "talk", "devops", null);
    expect(sessionsOf(twice)).toEqual([[1], [2], [4]]);
    expect(twice.inFront).toBe(tabOf(twice, 4));
  });

  it("moves it out of a split beside its session", () => {
    const beside = openBeside(twoSessions(), 4, askedBy, "talk");
    const tabs = moveToOwnTab(beside, 4, "talk", "devops", null);
    expect(sessionsOf(tabs)).toEqual([[1], [2], [4]]);
    expect(placedOf(tabs, 4, askedBy)).toBe("tab");
  });

  it("says nothing is placed of a session, or of a task with no pane", () => {
    const tabs = twoSessions();
    expect(placedOf(tabs, 1, askedBy)).toBeUndefined();
    expect(placedOf(tabs, 4, askedBy)).toBeUndefined();
  });
});

describe("sending a task back", () => {
  it("takes its tab off the strip and puts the front where a close would", () => {
    const own = moveToOwnTab(twoSessions(), 4, "talk", "devops", null);

    const tabs = sendBack(own, 4, oneWorkspace);

    expect(sessionsOf(tabs)).toEqual([[1], [2]]);
    // The tab beside it on the strip, as `closeTab` sends it.
    expect(tabs.inFront).toBe(closeTab(own, tabOf(own, 4), oneWorkspace).inFront);
    // It lives in its session's tab again, which shows what it showed.
    expect(homeOf(tabs, 4, askedBy)).toMatchObject({ tab: tabOf(tabs, 1), own: 1 });
    expect(visibleSessions(selectTab(tabs, tabOf(tabs, 1)))).toEqual([1]);
  });

  it("takes its pane out of a split, and the session's pane takes the room", () => {
    const beside = openBeside(twoSessions(), 4, askedBy, "talk");
    const tabs = sendBack(beside, 4, oneWorkspace);
    expect(sessionsOf(tabs)).toEqual([[1], [2]]);
    expect(tabs.byId[tabOf(tabs, 1)].layout.kind).toBe("pane");
    expect(focusedChat(tabs)).toBe(1);
  });

  it("changes nothing for a task with no pane", () => {
    const tabs = twoSessions();
    expect(sendBack(tabs, 4, oneWorkspace)).toBe(tabs);
  });
});

describe("opening a task beside its session", () => {
  it("splits the session's pane inside the session's tab, the task on the far side, focused", () => {
    const tabs = openBeside(selectTab(twoSessions(), 2), 4, askedBy, "talk");

    expect(sessionsOf(tabs)).toEqual([[1, 4], [2]]);
    expect(tabs.inFront).toBe(tabOf(tabs, 1));
    expect(visibleSessions(tabs)).toEqual([1, 4]);
    expect(focusedChat(tabs)).toBe(4);
    expect(placedOf(tabs, 4, askedBy)).toBe("beside");
    // The tab is still the session's.
    expect(chatOf(tabs, tabOf(tabs, 1))).toBe(1);
    const layout = tabs.byId[tabOf(tabs, 1)].layout;
    expect(layout.kind === "split" && layout.direction).toBe("row");
  });

  it("stops the session's pane showing the task: it shows its own chat beside it", () => {
    const shown = switchTabTo(twoSessions(), 4, askedBy);
    const tabs = openBeside(shown, 4, askedBy, "talk");
    expect(visibleSessions(tabs)).toEqual([1, 4]);
  });

  it("moves it out of a tab of its own, which goes", () => {
    const own = moveToOwnTab(twoSessions(), 4, "talk", "devops", null);
    const tabs = openBeside(own, 4, askedBy, "talk");
    expect(sessionsOf(tabs)).toEqual([[1, 4], [2]]);
    expect(tabs.inFront).toBe(tabOf(tabs, 1));
  });

  it("opens a task of a task beside the pane it lives in", () => {
    const tabs = openBeside(twoSessions(), 6, askedBy, "probe");
    expect(sessionsOf(tabs)).toEqual([[1, 6], [2]]);
    expect(sessionOf(tabs, 6, askedBy)?.own).toBe(1);
  });

  it("only focuses a task that is already beside its session", () => {
    const beside = openBeside(twoSessions(), 4, askedBy, "talk");
    const away = selectTab(beside, tabOf(beside, 2));
    const tabs = openBeside(away, 4, askedBy, "talk");
    expect(sessionsOf(tabs)).toEqual([[1, 4], [2]]);
    expect(tabs.inFront).toBe(tabOf(tabs, 1));
    expect(focusedChat(tabs)).toBe(4);
  });

  it("does nothing for a task whose session has no tab here", () => {
    const tabs = twoSessions();
    expect(openBeside(tabs, 12, askedBy, "left behind")).toBe(tabs);
  });

  it("is put back at a launch without taking the front or the keyboard", () => {
    const before = selectTab(twoSessions(), 2);
    const tabs = placeBeside(before, 4, askedBy, "talk");
    expect(sessionsOf(tabs)).toEqual([[1, 4], [2]]);
    expect(tabs.inFront).toBe(before.inFront);
    expect(tabs.byId[tabOf(tabs, 1)].focused).toBe(before.byId[tabOf(before, 1)].focused);
    // A task whose session did not come back stays in the list.
    expect(placeBeside(before, 12, askedBy, "left behind")).toBe(before);
  });
});

describe("the tasks of a session that have a pane of their own", () => {
  it("lists every one below it, at any depth, with where it is", () => {
    let tabs = moveToOwnTab(twoSessions(), 4, "talk", "devops", null);
    tabs = openBeside(tabs, 5, askedBy, "sweep");
    tabs = moveToOwnTab(tabs, 6, "probe", "devops", null);
    tabs = moveToOwnTab(tabs, 7, "notes", "devops", null);

    expect(tasksPlacedBelow(tabs, 1, askedBy)).toEqual([
      { session: 4, placed: "tab" },
      { session: 5, placed: "beside" },
      { session: 6, placed: "tab" },
    ]);
    expect(tasksPlacedBelow(tabs, 2, askedBy)).toEqual([{ session: 7, placed: "tab" }]);
  });
});

/** A listed chat: `parent` is the chat that started it, as a task. */
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

const chats = [
  listed(1, "steward 1"),
  listed(2, "steward 2"),
  listed(4, "talk", 1),
  listed(5, "sweep", 1),
  listed(6, "probe", 4, { workspace: "beta" }),
  listed(7, "notes", 2),
];

const said = (rows: readonly { name: string; level: number; placed?: string }[]) =>
  rows.map(
    (row) => `${"  ".repeat(row.level - 1)}${row.name}${row.placed ? ` [${row.placed}]` : ""}`,
  );

describe("the chats of a session's tab, when a task has a pane of its own", () => {
  it("still lists a task in a tab of its own under its session, marked, and not what is below it", () => {
    const tabs = moveToOwnTab(twoSessions(), 4, "talk", "devops", null);

    expect(said(chatsOfTab(tabs, tabOf(tabs, 1), chats))).toEqual([
      "steward 1",
      "  talk [tab]",
      "  sweep",
    ]);
    // Its own tab lists it as the tab's own chat, unmarked, with its tasks.
    expect(said(chatsOfTab(tabs, tabOf(tabs, 4), chats))).toEqual(["talk", "  probe"]);
    // And the two selectors agree.
    expect(chatsOfTabs(tabs, chats).get(tabOf(tabs, 1))).toEqual(
      chatsOfTab(tabs, tabOf(tabs, 1), chats),
    );
  });

  it("lists a task beside its session under it, marked, with its own tasks under it", () => {
    const tabs = openBeside(twoSessions(), 4, askedBy, "talk");

    expect(said(chatsOfTab(tabs, tabOf(tabs, 1), chats))).toEqual([
      "steward 1",
      "  talk [beside]",
      "    probe",
      "  sweep",
    ]);
  });

  it("keeps each pane's own chats for the pane: a task with a pane is its own pane's", () => {
    const tabs = openBeside(twoSessions(), 4, askedBy, "talk");
    const id = tabOf(tabs, 1);
    const [first, second] = panesOf(tabs, id);
    expect(chatsOfPane(tabs, id, first.pane, chats).map((row) => row.name)).toEqual([
      "steward 1",
      "sweep",
    ]);
    expect(chatsOfPane(tabs, id, second.pane, chats).map((row) => row.name)).toEqual([
      "talk",
      "probe",
    ]);
  });
});

describe("the breadcrumb of a pane that shows its own chat", () => {
  it("is the path down to a task in a tab of its own", () => {
    const tabs = moveToOwnTab(twoSessions(), 6, "probe", "devops", null);
    const id = tabOf(tabs, 6);
    const [pane] = panesOf(tabs, id);
    const crumbs = placedCrumbsOf(tabs, id, chats)[pane.pane];
    expect(crumbs.path.map((chat) => chat.name)).toEqual(["steward 1", "talk", "probe"]);
    expect(crumbs.elsewhere).toBe("beta");
    // Not the path of a pane switched to a task: that is `crumbsOf`'s.
    expect(crumbsOf(tabs, id, chats)).toEqual({});
  });

  it("is on both sides of a split: the session by its name, the task by its path", () => {
    const tabs = openBeside(twoSessions(), 4, askedBy, "talk");
    const id = tabOf(tabs, 1);
    const [first, second] = panesOf(tabs, id);
    const crumbs = placedCrumbsOf(tabs, id, chats);
    expect(crumbs[first.pane].path.map((chat) => chat.name)).toEqual(["steward 1"]);
    expect(crumbs[second.pane].path.map((chat) => chat.name)).toEqual(["steward 1", "talk"]);
  });

  it("is nothing for a session with no task beside it", () => {
    const tabs = twoSessions();
    expect(placedCrumbsOf(tabs, tabOf(tabs, 1), chats)).toEqual({});
  });

  it("is nothing where the chat that asked is gone: a name alone is no path", () => {
    const orphan = listed(12, "left behind", 9);
    const tabs = moveToOwnTab(twoSessions(), 12, "left behind", "devops", null);
    expect(placedCrumbsOf(tabs, tabOf(tabs, 12), [...chats, orphan])).toEqual({});
  });
});
