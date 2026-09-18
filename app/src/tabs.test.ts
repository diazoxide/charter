import { describe, expect, it } from "vitest";
import {
  closeFocusedPane,
  closeTab,
  focusPane,
  noTabs,
  openTab,
  panesOf,
  selectTab,
  splitFocusedPane,
  visibleSessions,
  type Tabs,
} from "./tabs";

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
    const tabs = closeFocusedPane(splitFocusedPane(openTab(noTabs(), 11), "row", 12));
    const tab = tabs.byId[front(tabs)];

    expect(tab.layout).toEqual({ kind: "pane", pane: 1, session: 11 });
    expect(tab.focused).toBe(1);
    expect(visibleSessions(tabs)).toEqual([11]);
  });

  it("closes the tab when its last pane closes", () => {
    const tabs = closeFocusedPane(openTab(noTabs(), 11));

    expect(tabs.order).toEqual([]);
    expect(tabs.inFront).toBeUndefined();
    expect(visibleSessions(tabs)).toEqual([]);
  });

  it("brings the tab beside it to the front when a tab closes", () => {
    const tabs = twoTabs();

    const left = closeTab(tabs, front(tabs));

    expect(visibleSessions(left)).toEqual([11]);
  });

  it("keeps the tab in front when another tab closes", () => {
    const tabs = twoTabs();

    const left = closeTab(tabs, tabs.order[0]);

    expect(left.order.length).toBe(1);
    expect(visibleSessions(left)).toEqual([22]);
  });

  it("gives every pane its own id, so a pane is never confused with a closed one", () => {
    const opened = splitFocusedPane(openTab(noTabs(), 11), "row", 12);

    const reopened = splitFocusedPane(closeFocusedPane(opened), "row", 13);

    expect(panesOf(reopened, front(reopened))).toEqual([
      { pane: 1, session: 11 },
      { pane: 3, session: 13 },
    ]);
  });

  it("leaves tabs alone when something that is not there is closed or focused", () => {
    const tabs = openTab(noTabs(), 11);

    expect(closeTab(tabs, 404)).toEqual(tabs);
    expect(focusPane(tabs, 404)).toEqual(tabs);
    expect(selectTab(tabs, 404)).toEqual(tabs);
    expect(closeFocusedPane(noTabs())).toEqual(noTabs());
    expect(splitFocusedPane(noTabs(), "row", 11)).toEqual(noTabs());
  });
});
