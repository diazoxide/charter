import { describe, expect, it } from "vitest";
import {
  chatNameOf,
  byLastActivity,
  closeFocusedPane,
  closeTab,
  focusPane,
  noTabs,
  openTab,
  openTabBehind,
  panesOf,
  selectTab,
  showWorkspace,
  movedAt,
  splitFocusedPane,
  tabsIn,
  visibleSessions,
  workspaceOf,
  type FiledIn,
  type Tabs,
  chatOf,
  focusedContent,
  openView,
  putViewBack,
  refileViews,
  stopWaiting,
  viewKey,
  type ViewRef,
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

/** A pane showing a chat, as a layout holds one. Every chat here is the first tab's, `1`. */
function chatPane(pane: number, session: number) {
  return { kind: "pane", pane, content: { kind: "session", session, chat: "1" } };
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

    expect(chatNameOf(tabs, tabs.order[0])).toBe("3");
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
      children: [chatPane(1, 11), chatPane(2, 12)],
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
      children: [chatPane(1, 11), chatPane(3, 13)],
    });
  });

  it("puts what is left in a split's place when the focused pane closes", () => {
    const tabs = closeFocusedPane(splitFocusedPane(openTab(noTabs(), 11), "row", 12), oneWorkspace);
    const tab = tabs.byId[front(tabs)];

    expect(tab.layout).toEqual(chatPane(1, 11));
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

/**
 * The workspace strip's per-workspace counts, at the scale the limits are written for.
 *
 * charter-app#133 asked for this to be MEASURED before anything was changed. Its author's own
 * review of #131 named it as the reviewer's first target: `tabsIn` is called once per
 * workspace inside the render body, unmemoised, and every call scans every tab, and every tab
 * asks `filedIn`, which scans the sidebar. Ten workspaces × fifty tabs × a fifty-chat scan was
 * called "~25k comparisons on every `chat-moved` event", and `chat-moved` arrives on every
 * hook report from every chat.
 *
 * **Measured, on the shape below and on the machine CI runs on:**
 *
 * | workspaces × chats | the strip's counts | the whole window, per `chat-moved` (jsdom) |
 * |--------------------|--------------------|--------------------------------------------|
 * | 10 × 50 (ADR 0026) | **0.022 ms**       | 4.4 ms                                     |
 * | 10 × 50, all fifty asking | 0.041 ms    | —                                          |
 * | 10 × 100           | 0.071 ms           | —                                          |
 * | 20 × 200           | 0.43 ms            | —                                          |
 * | 50 × 500           | 6.2 ms             | —                                          |
 *
 * **Nothing was changed, and no `useMemo` was added.** At the limit the loop is 22 µs: one
 * eight-hundredth of a 16.7 ms frame, and half a percent of the re-render it sits inside. A
 * memo would save 22 µs and cost a dependency array over `tabs`, which `chat-moved` does not
 * even change — so the memo would hit on exactly the event it was proposed for and be paid
 * for on every other one. CLAUDE.md is explicit: optimise only against the spec's limits.
 *
 * The growth is real and it is quadratic — 50 × 500 is 6.2 ms — but that is ten times the
 * product's scale on both axes at once. What follows is the measurement kept as an assertion,
 * the way #104 kept the palette's: a MILLISECOND assertion would be flaky on a shared runner,
 * so what is pinned is the work itself. A third nested scan moves these numbers and fails
 * here, rather than being a surprise at fifty chats.
 */
describe("the workspace strip at fifty chats (charter-app#133)", () => {
  const WORKSPACES = 10;
  const CHATS = 50;

  /** A plane the size of ADR 0026's limits: fifty chats spread over ten workspaces. */
  function plane() {
    const names = Array.from({ length: WORKSPACES }, (_, w) => `ws-${w}`);
    const where = new Map<number, string>();
    let tabs = noTabs();
    for (let i = 0; i < CHATS; i++) {
      where.set(100 + i, names[i % WORKSPACES]);
      tabs = openTab(tabs, 100 + i, `${names[i % WORKSPACES]}.${i + 1}`);
    }
    return { names, where, tabs };
  }

  /** `PlaneView`'s `filedIn`, which scans the sidebar's chats — and counts what it touched. */
  function counting(where: Map<number, string>) {
    const count = { calls: 0, comparisons: 0 };
    const chats = [...where.entries()];
    const filedIn: FiledIn = (session) => {
      count.calls += 1;
      for (const [known, workspace] of chats) {
        count.comparisons += 1;
        if (known === session) return workspace;
      }
      return "Outside every workspace";
    };
    return { count, filedIn };
  }

  /** What the render body does for the strip, and nothing else: per workspace, how many chats
   *  are open there and how many of them are asking for you. */
  function strip(tabs: Tabs, names: string[], filedIn: FiledIn, needsYou: number[]) {
    return names.map((workspace) => ({
      here: tabsIn(tabs, workspace, filedIn).length,
      waiting: needsYou.filter((session) => filedIn(session) === workspace).length,
    }));
  }

  it("counts every chat once per workspace, and the counts are the ones drawn", () => {
    const { names, where, tabs } = plane();
    const { filedIn } = counting(where);

    const drawn = strip(tabs, names, filedIn, [100, 105]);

    expect(drawn.map((one) => one.here)).toEqual(Array.from({ length: WORKSPACES }, () => 5));
    // Chat 100 is in `ws-0` and chat 105 in `ws-5`, so one mark each and none anywhere else.
    expect(drawn.map((one) => one.waiting)).toEqual([1, 0, 0, 0, 0, 1, 0, 0, 0, 0]);
  });

  it("costs one sidebar scan per tab per workspace, which is what #133 asked for in numbers", () => {
    const { names, where, tabs } = plane();
    const { count, filedIn } = counting(where);

    strip(tabs, names, filedIn, []);

    // 10 workspaces × 50 tabs. This is the loop the review called quadratic, and it is.
    expect(count.calls).toBe(WORKSPACES * CHATS);
    // And each call walks the sidebar as far as the chat it is asking about, which averages
    // half of it: 12,750 comparisons, not the 25,000 the review feared. **At 0.022 ms it
    // needs no memo.** What this pins is that nothing has added a third nesting since.
    expect(count.comparisons).toBe(12_750);
  });

  it("asks nothing extra of the workspaces a chat is not in", () => {
    // The needs-you mark is the other per-workspace scan, and it is over the QUEUE, not over
    // the chats: fifty chats all asking at once is fifty calls per workspace, not 2,500.
    const { names, where, tabs } = plane();
    const { count, filedIn } = counting(where);
    const everyone = [...where.keys()];

    strip(tabs, names, filedIn, everyone);

    expect(count.calls).toBe(WORKSPACES * CHATS + WORKSPACES * everyone.length);
  });
});

describe("the order of the strip and the order of the menu", () => {
  /** Four tabs, opened in order: 1, 2, 3, 4. */
  function fourTabs(): Tabs {
    return [11, 22, 33, 44].reduce((tabs, session) => openTab(tabs, session), noTabs());
  }

  /** When each chat last moved, as the core counts it. Anything not named has never moved. */
  const moves =
    (when: Record<number, number>) =>
    (session: number): number =>
      when[session] ?? 0;

  it("appends a new tab and never puts it anywhere else", () => {
    // ADR 0039 ratifies what the code already does, and this is the guard on it. The change
    // it exists to refuse is small, helpful-looking, and will be proposed the first time
    // somebody has fifty tabs and cannot find one.
    const tabs = fourTabs();

    expect(tabs.order).toEqual([1, 2, 3, 4]);
    expect(openTab(tabs, 55).order).toEqual([1, 2, 3, 4, 5]);
  });

  it("does not move a tab when its chat is the busiest thing on the plane", () => {
    // The whole of the fixed-order rule: activity is an input to the MENU and to nothing on
    // the strip. There is no call here that could move it, and that is the assertion.
    const tabs = fourTabs();

    const busy = moves({ 11: 99 });

    expect(tabsIn(tabs, "alpha", oneWorkspace)).toEqual([1, 2, 3, 4]);
    expect(movedAt(tabs, 1, busy)).toBe(99);
  });

  it("lists what it is given most recently moved first", () => {
    const tabs = fourTabs();

    expect(byLastActivity([1, 2, 3, 4], tabs, moves({ 11: 3, 22: 1, 33: 4, 44: 2 }))).toEqual([
      3, 1, 4, 2,
    ]);
  });

  it("keeps the order it was given where nothing tells them apart", () => {
    // At a launch every tab reads 0. A menu that shuffled them would be a list whose rows
    // move between two openings for no reason the operator can see.
    const tabs = fourTabs();

    expect(byLastActivity([1, 2, 3, 4], tabs, moves({}))).toEqual([1, 2, 3, 4]);
    expect(byLastActivity([1, 2, 3, 4], tabs, moves({ 11: 5, 33: 5 }))).toEqual([1, 3, 2, 4]);
  });

  it("puts a chat nothing has been heard about last", () => {
    // `0` is "never heard from", which is honest and is not "moved at the beginning of time
    // and therefore first".
    const tabs = fourTabs();

    expect(byLastActivity([1, 2], tabs, moves({ 22: 1 }))).toEqual([2, 1]);
  });

  it("takes a split tab's most recent pane, not its first", () => {
    // A tab holds one chat until it is split, and then it holds two. The tab last moved when
    // the later of them did — a tab whose second pane is working is not an idle tab.
    const tabs = splitFocusedPane(openTab(noTabs(), 11), "row", 22);

    expect(movedAt(tabs, 1, moves({ 11: 1, 22: 7 }))).toBe(7);
    expect(movedAt(tabs, 1, moves({ 11: 7, 22: 1 }))).toBe(7);
  });

  it("leaves the list it was given alone", () => {
    // It is handed the strip's own order, which nothing may re-sort in place.
    const tabs = fourTabs();
    const strip = [1, 2, 3, 4];

    byLastActivity(strip, tabs, moves({ 44: 9 }));

    expect(strip).toEqual([1, 2, 3, 4]);
  });
});

describe("a pin on the strip", () => {
  /** Four tabs, opened 1, 2, 3, 4, with the second in front. */
  function fourTabs(): Tabs {
    return selectTab(
      [11, 22, 33, 44].reduce((tabs, s) => openTab(tabs, s), noTabs()),
      2,
    );
  }

  /** The tabs the operator pinned, by tab id. */
  const holding =
    (...ids: number[]) =>
    (id: number) =>
      ids.includes(id);

  it("draws a pinned tab first and leaves the rest where they were", () => {
    const tabs = fourTabs();

    expect(tabsIn(tabs, "alpha", oneWorkspace, holding(3))).toEqual([3, 1, 2, 4]);
  });

  it("does not sort two pinned tabs against each other", () => {
    // A pin says WHICH tabs come first, never in what order they do. Sorting them would be
    // the strip deciding something the operator did not, which is what ADR 0039 refuses.
    const tabs = fourTabs();

    expect(tabsIn(tabs, "alpha", oneWorkspace, holding(4, 2))).toEqual([2, 4, 1, 3]);
  });

  it("leaves tabs.order alone, which is the tab's real place", () => {
    const tabs = fourTabs();

    tabsIn(tabs, "alpha", oneWorkspace, holding(4));

    expect(tabs.order).toEqual([1, 2, 3, 4]);
  });

  it("brings forward the tab beside it ON THE STRIP when one closes", () => {
    // "Beside" is what the operator can see. With tab 4 pinned the strip reads 4, 1, 2, 3 —
    // so closing tab 1 comes back to tab 4 and not to "nothing before it".
    const tabs = selectTab(fourTabs(), 1);

    const after = closeTab(tabs, 1, oneWorkspace, holding(4));

    expect(after.inFront).toBe(4);
  });

  it("comes back to the strip's first tab, not the order's, when a workspace is focused", () => {
    const tabs = fourTabs();

    expect(showWorkspace(tabs, "alpha", oneWorkspace, undefined, holding(3)).inFront).toBe(3);
  });

  it("is what a strip with nothing pinned always did", () => {
    // Every caller that does not care about pins passes nothing, and gets the old answer.
    const tabs = fourTabs();

    expect(tabsIn(tabs, "alpha", oneWorkspace)).toEqual([1, 2, 3, 4]);
    expect(closeTab(selectTab(tabs, 2), 2, oneWorkspace).inFront).toBe(1);
  });
});

describe("a tab a handoff opened (charter-app#204)", () => {
  it("goes on the strip without taking the front from the chat being read", () => {
    const reading = openTab(noTabs(), 1, "1");

    const after = openTabBehind(reading, 7, "handoff from 1");

    expect(after.inFront).toBe(reading.inFront);
    expect(panesOf(after, after.order[1])).toEqual([{ pane: 2, session: 7 }]);
    expect(chatNameOf(after, after.order[1])).toBe("handoff from 1");
  });

  it("takes the front of a window with nothing in front, where there is nothing to interrupt", () => {
    const after = openTabBehind(noTabs(), 7, "handoff from 1");

    expect(after.inFront).toBe(after.order[0]);
  });
});

describe("a tab that shows a view (ADR 0043, as amended 2026-09-23)", () => {
  const STEWARD: ViewRef = { from: null, view: "persona", key: "steward" };
  const STATISTICS: ViewRef = { from: "persona-statistics", view: "statistics", key: "" };
  const filed = () => "alpha";

  it("opens in front, on the strip it was opened from, named by the view", () => {
    const tabs = openView(openTab(noTabs(), 11), STEWARD, "steward", "beta");
    const id = front(tabs);

    expect(tabs.byId[id].name).toBe("steward");
    expect(workspaceOf(tabs, id, filed)).toBe("beta");
    expect(tabsIn(tabs, "beta", filed)).toEqual([id]);
  });

  it("has no chat of its own, so nothing pretends it does", () => {
    const tabs = openView(noTabs(), STEWARD, "steward", "alpha");
    const id = front(tabs);

    expect(chatOf(tabs, id)).toBeUndefined();
    expect(panesOf(tabs, id)).toEqual([]);
    expect(visibleSessions(tabs)).toEqual([]);
    expect(focusedContent(tabs)).toEqual({ kind: "view", view: STEWARD, workspace: "alpha" });
  });

  it("is brought forward, not opened twice, when the same view is opened again", () => {
    const once = openView(openTab(noTabs(), 11), STEWARD, "steward", "alpha");
    const back = selectTab(once, once.order[0]);

    const again = openView(back, STEWARD, "steward", "beta");

    expect(again.order).toEqual(once.order);
    expect(again.inFront).toBe(once.inFront);
    // Where it already was, and not moved to the strip it was asked for from a second time.
    expect(workspaceOf(again, front(again), filed)).toBe("alpha");
  });

  it("tells two views apart by who draws them and what they are about", () => {
    const tabs = [STEWARD, { ...STEWARD, key: "release" }, STATISTICS].reduce(
      (open, view) => openView(open, view, viewKey(view), "alpha"),
      noTabs(),
    );

    expect(tabs.order).toHaveLength(3);
    expect(viewKey(STATISTICS)).toBe("ext/persona-statistics/statistics/");
    expect(viewKey(STEWARD)).toBe("charter/persona/steward");
  });

  it("finds a view on the far side of a split, and focuses that pane", () => {
    const beside = splitFocusedPane(openView(noTabs(), STEWARD, "steward", "alpha"), "row", 11);
    const away = openTab(beside, 22);

    const back = openView(away, STEWARD, "steward", "alpha");

    expect(back.inFront).toBe(beside.inFront);
    expect(back.byId[front(back)].focused).toBe(1);
  });

  it("gives a chat split beside it a name of its own, not the view's", () => {
    const tabs = splitFocusedPane(openView(noTabs(), STEWARD, "steward", "alpha"), "row", 11, "2");

    expect(chatNameOf(tabs, front(tabs))).toBe("2");
    expect(panesOf(tabs, front(tabs))).toEqual([{ pane: 2, session: 11 }]);
    // The tab is still the view's: its first pane says what it is.
    expect(chatOf(tabs, front(tabs))).toBeUndefined();
  });

  it("closes with its last pane and leaves no chat behind to end", () => {
    const tabs = openView(openTab(noTabs(), 11), STEWARD, "steward", "alpha");

    const closed = closeFocusedPane(tabs, filed);

    expect(closed.order).toEqual([tabs.order[0]]);
    expect(closed.inFront).toBe(tabs.order[0]);
  });

  it("keeps the fixed order: appended when opened, and never moved (ADR 0039)", () => {
    const tabs = openTab(openView(openTab(noTabs(), 11), STEWARD, "steward", "alpha"), 22);

    expect(tabs.order).toEqual([1, 2, 3]);
    expect(selectTab(tabs, 2).order).toEqual([1, 2, 3]);
  });

  describe("put back by a launch", () => {
    it("goes back at the place it had, behind whatever is in front", () => {
      const chats = openTab(openTab(noTabs(), 11), 22);

      const tabs = putViewBack(chats, STEWARD, "steward", "alpha", 1);

      expect(tabs.order).toEqual([1, 3, 2]);
      expect(tabs.inFront).toBe(chats.inFront);
    });

    it("goes at the end when the place it had is past the end", () => {
      const tabs = putViewBack(openTab(noTabs(), 11), STEWARD, "steward", "alpha", 9);

      expect(tabs.order).toEqual([1, 2]);
    });

    it("waits for a press before it is asked anything, and stops waiting on one", () => {
      const tabs = selectTab(putViewBack(noTabs(), STATISTICS, "Statistics", "alpha", 0), 1);

      expect(focusedContent(tabs)).toMatchObject({ kind: "view", waits: true });
      expect(focusedContent(stopWaiting(tabs, 1))).toMatchObject({ waits: false });
    });

    it("stops waiting when the operator opens the same view, which is the press", () => {
      const tabs = openView(
        putViewBack(openTab(noTabs(), 11), STATISTICS, "Statistics", "alpha", 1),
        STATISTICS,
        "Statistics",
        "alpha",
      );

      expect(focusedContent(tabs)).toMatchObject({ kind: "view", waits: false });
    });

    it("is not drawn twice when the view is already open", () => {
      const open = openView(noTabs(), STEWARD, "steward", "alpha");

      expect(putViewBack(open, STEWARD, "steward", "alpha", 0)).toBe(open);
    });
  });

  it("moves to the strip outside every workspace when its own workspace goes", () => {
    const tabs = openView(noTabs(), STEWARD, "steward", "gone");

    const refiled = refileViews(tabs, (name) => name === "alpha", "outside");

    expect(workspaceOf(refiled, front(refiled), filed)).toBe("outside");
    // And nothing at all when every view's workspace is still there, so a caller can tell.
    expect(refileViews(refiled, () => true, "outside")).toBe(refiled);
  });
});
