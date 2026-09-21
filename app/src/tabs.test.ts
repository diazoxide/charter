import { describe, expect, it } from "vitest";
import {
  closeFocusedPane,
  closeTab,
  focusPane,
  noTabs,
  openTab,
  panesOf,
  selectTab,
  showWorkspace,
  splitFocusedPane,
  tabsIn,
  visibleSessions,
  workspaceOf,
  type FiledIn,
  type Tabs,
} from "./tabs";

/** A plane charter has not read yet files every chat the same way, which is the shape every
 *  test written before workspaces were an axis was written against. */
const oneWorkspace: FiledIn = () => "alpha";

/** The tab on screen, which every test below has. */
function front(tabs: Tabs): number {
  if (tabs.inFront === undefined) throw new Error("no tab is in front");
  return tabs.inFront;
}

/** Two tabs, each showing one session: the second is the one in front. */
function twoTabs(): Tabs {
  return openTab(openTab(noTabs(), 11), 22);
}

describe("a tab's name", () => {
  it("is what the chat it shows is called, so a reopened tab is recognisable", () => {
    // A tab numbered 1 says nothing about which chat came back; `ide.7` does.
    const tabs = openTab(noTabs(), 7, "ide.7");

    expect(tabs.byId[tabs.order[0]].name).toBe("ide.7");
  });

  it("falls back to the tab's own number for a chat with no name", () => {
    const tabs = openTab(noTabs(), 7, "");

    expect(tabs.byId[tabs.order[0]].name).toBe("1");
  });

  it("carries the persona the chat adopted, because a number identifies nothing", () => {
    // charter-app#130: at fifty chats the strip read `3`, `4`, `5`. A number is what a chat
    // is called to charter; it tells the operator nothing about which chat it is.
    const tabs = openTab(noTabs(), 7, "3", "steward");

    expect(tabs.byId[tabs.order[0]].name).toBe("3 steward");
  });

  it("keeps the chat's own name beside it, for the core and for a split", () => {
    // The core is told what a chat is CALLED, and a split's chat is called what the tab's
    // chat is called. The persona is the operator's choice, not part of the chat's name.
    const tabs = openTab(noTabs(), 7, "3", "steward");

    expect(tabs.byId[tabs.order[0]].chat).toBe("3");
  });

  it("says only the chat's name when charter knows no persona for it", () => {
    const tabs = openTab(noTabs(), 7, "3", null);

    expect(tabs.byId[tabs.order[0]].name).toBe("3");
  });
});

describe("tabs on a workspace's strip", () => {
  /** Three chats: two in `alpha`, one in `beta`. */
  const spread: FiledIn = (session) => (session === 22 ? "beta" : "alpha");
  const three = () => openTab(openTab(openTab(noTabs(), 11), 22), 33);

  it("shows one workspace's chats, which is the axis the tmux frame had", () => {
    expect(tabsIn(three(), "alpha", spread)).toEqual([1, 3]);
    expect(tabsIn(three(), "beta", spread)).toEqual([2]);
  });

  it("files a tab by the chat of its first pane", () => {
    const tabs = three();

    expect(workspaceOf(tabs, 2, spread)).toBe("beta");
    expect(workspaceOf(tabs, 404, spread)).toBeUndefined();
  });

  it("puts one of the focused workspace's own chats in front", () => {
    const shown = showWorkspace(three(), "alpha", spread);

    expect(shown.inFront).toBe(1);
  });

  it("comes back to the chat that was in front on that strip, not to its first", () => {
    const shown = showWorkspace(three(), "alpha", spread, 3);

    expect(shown.inFront).toBe(3);
  });

  it("ignores a remembered tab that is not on that strip any more", () => {
    // It can have been ended, or it can belong to another workspace entirely. Either way
    // honouring it would put a chat the strip does not show on screen.
    expect(showWorkspace(three(), "alpha", spread, 2).inFront).toBe(1);
    expect(showWorkspace(three(), "alpha", spread, 404).inFront).toBe(1);
  });

  it("puts NOTHING in front for a workspace with no chats", () => {
    // Leaving another workspace's chat on screen under this workspace's empty strip would
    // be the app showing a chat the strip says is not there.
    expect(showWorkspace(three(), "gamma", spread).inFront).toBeUndefined();
  });

  it("changes nothing else about the arrangement", () => {
    const tabs = three();

    expect(showWorkspace(tabs, "beta", spread).order).toEqual(tabs.order);
    expect(showWorkspace(tabs, "gamma", spread).byId).toEqual(tabs.byId);
  });

  it("brings forward the tab beside it IN ITS OWN WORKSPACE when one closes", () => {
    // Reaching across to a tab on another strip would move the operator to a workspace they
    // did not ask for, and leave the strip they are looking at with nothing selected.
    const tabs = selectTab(three(), 3);

    expect(closeTab(tabs, 3, spread).inFront).toBe(1);
  });

  it("puts nothing in front when the last chat in a workspace closes", () => {
    const tabs = selectTab(three(), 2);

    expect(closeTab(tabs, 2, spread).inFront).toBeUndefined();
  });

  it("leaves the front tab alone when a chat in another workspace closes", () => {
    const tabs = selectTab(three(), 3);

    expect(closeTab(tabs, 2, spread).inFront).toBe(3);
  });
});

describe("tabs and their panes", () => {
  it("opens a tab showing one session, in front", () => {
    const tabs = openTab(noTabs(), 11);

    expect(tabs.order.length).toBe(1);
    expect(panesOf(tabs, front(tabs))).toEqual([{ pane: 1, session: 11 }]);
    expect(visibleSessions(tabs)).toEqual([11]);
  });

  it("shows only the sessions of the tab in front, so hidden panes draw nothing", () => {
    const tabs = twoTabs();

    expect(visibleSessions(tabs)).toEqual([22]);
  });

  it("brings a tab to the front when it is selected", () => {
    const tabs = twoTabs();
    const behind = tabs.order[0];

    expect(visibleSessions(selectTab(tabs, behind))).toEqual([11]);
  });

  it("splits the focused pane into two, side by side, the new one focused", () => {
    const tabs = splitFocusedPane(openTab(noTabs(), 11), "row", 12);
    const tab = tabs.byId[front(tabs)];

    expect(panesOf(tabs, front(tabs))).toEqual([
      { pane: 1, session: 11 },
      { pane: 2, session: 12 },
    ]);
    expect(tab.layout).toEqual({
      kind: "split",
      direction: "row",
      children: [
        { kind: "pane", pane: 1, session: 11 },
        { kind: "pane", pane: 2, session: 12 },
      ],
    });
    expect(tab.focused).toBe(2);
  });

  it("splits the focused pane and not another one", () => {
    const two = splitFocusedPane(openTab(noTabs(), 11), "row", 12);
    const onFirst = focusPane(two, 1);

    const tabs = splitFocusedPane(onFirst, "column", 13);

    expect(panesOf(tabs, front(tabs)).map((pane) => pane.session)).toEqual([11, 13, 12]);
    const layout = tabs.byId[front(tabs)].layout;
    expect(layout.kind === "split" && layout.children[0]).toEqual({
      kind: "split",
      direction: "column",
      children: [
        { kind: "pane", pane: 1, session: 11 },
        { kind: "pane", pane: 3, session: 13 },
      ],
    });
  });

  it("puts what is left in a split's place when the focused pane closes", () => {
    const tabs = closeFocusedPane(splitFocusedPane(openTab(noTabs(), 11), "row", 12), oneWorkspace);
    const tab = tabs.byId[front(tabs)];

    expect(tab.layout).toEqual({ kind: "pane", pane: 1, session: 11 });
    expect(tab.focused).toBe(1);
    expect(visibleSessions(tabs)).toEqual([11]);
  });

  it("closes the tab when its last pane closes", () => {
    const tabs = closeFocusedPane(openTab(noTabs(), 11), oneWorkspace);

    expect(tabs.order).toEqual([]);
    expect(tabs.inFront).toBeUndefined();
    expect(visibleSessions(tabs)).toEqual([]);
  });

  it("brings the tab beside it to the front when a tab closes", () => {
    const tabs = twoTabs();

    const left = closeTab(tabs, front(tabs), oneWorkspace);

    expect(visibleSessions(left)).toEqual([11]);
  });

  it("keeps the tab in front when another tab closes", () => {
    const tabs = twoTabs();

    const left = closeTab(tabs, tabs.order[0], oneWorkspace);

    expect(left.order.length).toBe(1);
    expect(visibleSessions(left)).toEqual([22]);
  });

  it("gives every pane its own id, so a pane is never confused with a closed one", () => {
    const opened = splitFocusedPane(openTab(noTabs(), 11), "row", 12);

    const reopened = splitFocusedPane(closeFocusedPane(opened, oneWorkspace), "row", 13);

    expect(panesOf(reopened, front(reopened))).toEqual([
      { pane: 1, session: 11 },
      { pane: 3, session: 13 },
    ]);
  });

  it("leaves tabs alone when something that is not there is closed or focused", () => {
    const tabs = openTab(noTabs(), 11);

    expect(closeTab(tabs, 404, oneWorkspace)).toEqual(tabs);
    expect(focusPane(tabs, 404)).toEqual(tabs);
    expect(selectTab(tabs, 404)).toEqual(tabs);
    expect(closeFocusedPane(noTabs(), oneWorkspace)).toEqual(noTabs());
    expect(splitFocusedPane(noTabs(), "row", 11)).toEqual(noTabs());
  });
});
