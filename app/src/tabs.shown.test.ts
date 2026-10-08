import { describe, expect, it } from "vitest";
import {
  closeChat,
  closeFocusedPane,
  focusedChat,
  focusPane,
  homeOf,
  keepShown,
  layoutShown,
  noTabs,
  openTab,
  panesOf,
  replaceSession,
  restoreShown,
  selectTab,
  showOwn,
  showOwnIn,
  shownIn,
  shownLive,
  splitFocusedPane,
  switchTabTo,
  taskShownIn,
  visibleSessions,
  type AskedBy,
  type FiledIn,
  type Tabs,
} from "./tabs";

/**
 * **What a session's tab shows** (#1486): the session's own chat, or one of the tasks below it.
 * The core keeps who asked whom, and it is handed in here as a function, as where a chat is
 * filed is.
 */

const oneWorkspace: FiledIn = () => "alpha";

/** Who asked whom, as the core lists it: each chat by the chat that started it. */
const lineage =
  (asked: Record<number, number>): AskedBy =>
  (session) =>
    asked[session];

/** Chat 1 dispatched 4 and 5; 4 dispatched 6; chat 2 dispatched 7. */
const askedBy = lineage({ 4: 1, 5: 1, 6: 4, 7: 2 });

/** Two sessions, each in a tab of its own: 1 and 2. The second is in front. */
function twoSessions(): Tabs {
  return openTab(openTab(noTabs(), 1), 2);
}

/** The id of the tab whose own chat is `session`. */
function tabOf(tabs: Tabs, session: number): number {
  const found = tabs.order.find((id) => panesOf(tabs, id).some((one) => one.session === session));
  if (found === undefined) throw new Error(`no tab is session ${session}'s`);
  return found;
}

/** What each pane of the tab in front shows, left to right. */
const onScreen = (tabs: Tabs) => visibleSessions(tabs);

describe("a task's home", () => {
  it("is the tab of the session that asked for it", () => {
    const tabs = twoSessions();
    expect(homeOf(tabs, 4, askedBy)).toMatchObject({ tab: tabOf(tabs, 1), own: 1 });
    expect(homeOf(tabs, 7, askedBy)).toMatchObject({ tab: tabOf(tabs, 2), own: 2 });
  });

  it("is, for a task of a task, the tab of the session at the top", () => {
    const tabs = twoSessions();
    expect(homeOf(tabs, 6, askedBy)).toMatchObject({ tab: tabOf(tabs, 1), own: 1 });
  });

  it("is a chat's own tab when it has one, whoever asked for it", () => {
    // Chat 4 was given a tab of its own: it and what it started live there.
    const tabs = openTab(twoSessions(), 4);
    expect(homeOf(tabs, 4, askedBy)).toMatchObject({ tab: tabOf(tabs, 4), own: 4 });
    expect(homeOf(tabs, 6, askedBy)).toMatchObject({ tab: tabOf(tabs, 4), own: 4 });
    expect(homeOf(tabs, 5, askedBy)).toMatchObject({ tab: tabOf(tabs, 1), own: 1 });
  });

  it("is nowhere when the session that asked has no tab in this window", () => {
    const tabs = twoSessions();
    expect(homeOf(tabs, 9, lineage({ 9: 8 }))).toBeUndefined();
    expect(homeOf(tabs, 9, lineage({}))).toBeUndefined();
  });

  it("is found in a lineage that loops back on itself, without going round it", () => {
    expect(homeOf(twoSessions(), 8, lineage({ 8: 9, 9: 8 }))).toBeUndefined();
  });
});

describe("switching a tab to a chat", () => {
  it("shows the task in its session's tab, brings that tab forward, and adds no tab", () => {
    const tabs = twoSessions();
    const shown = switchTabTo(tabs, 4, askedBy);

    expect(shown.order).toEqual(tabs.order);
    expect(shown.inFront).toBe(tabOf(tabs, 1));
    expect(onScreen(shown)).toEqual([4]);
    expect(taskShownIn(shown, tabOf(tabs, 1))).toBe(4);
    // The tab is still the session's: what a close ends, and what the strip counts.
    expect(panesOf(shown, tabOf(tabs, 1)).map((one) => one.session)).toEqual([1]);
  });

  it("shows another task in place of the first, and the session's own chat again", () => {
    const tabs = twoSessions();
    const first = switchTabTo(tabs, 4, askedBy);
    const second = switchTabTo(first, 5, askedBy);
    expect(onScreen(second)).toEqual([5]);

    const own = switchTabTo(second, 1, askedBy);
    expect(onScreen(own)).toEqual([1]);
    expect(taskShownIn(own, tabOf(tabs, 1))).toBeUndefined();
    // Nothing is left behind on the tab: it is the tab it was before any task was shown.
    expect(own.byId[tabOf(tabs, 1)]).toEqual(selectTab(tabs, tabOf(tabs, 1)).byId[tabOf(tabs, 1)]);
  });

  it("shows a task of a task in the tab of the session at the top", () => {
    const shown = switchTabTo(twoSessions(), 6, askedBy);
    expect(onScreen(shown)).toEqual([6]);
    expect(shown.order).toHaveLength(2);
  });

  it("changes nothing for a chat whose session has no tab here", () => {
    const tabs = twoSessions();
    expect(switchTabTo(tabs, 9, lineage({ 9: 8 }))).toBe(tabs);
  });

  it("changes nothing when the tab is in front and already shows the chat", () => {
    const shown = switchTabTo(twoSessions(), 4, askedBy);
    expect(switchTabTo(shown, 4, askedBy)).toBe(shown);
  });

  it("is remembered by each tab while another is in front", () => {
    const tabs = twoSessions();
    const one = switchTabTo(tabs, 4, askedBy);
    const two = switchTabTo(one, 7, askedBy);
    expect(onScreen(two)).toEqual([7]);

    // Back on the first tab, by the strip: it shows the task it was left on.
    const back = selectTab(two, tabOf(tabs, 1));
    expect(onScreen(back)).toEqual([4]);
    expect(onScreen(selectTab(back, tabOf(tabs, 2)))).toEqual([7]);
  });

  it("shows a task in the pane of the session that asked, in a tab that is split", () => {
    // Tab of 1, split: chat 3 beside it. Chat 3 dispatched 8.
    const split = splitFocusedPane(selectTab(twoSessions(), 1), "row", 3);
    const asked = lineage({ 4: 1, 8: 3 });

    const shown = switchTabTo(split, 4, asked);
    expect(onScreen(shown)).toEqual([4, 3]);
    // The pane the task is shown in has the keyboard.
    expect(focusedChat(shown)).toBe(4);

    const both = switchTabTo(shown, 8, asked);
    expect(onScreen(both)).toEqual([4, 8]);
    expect(focusedChat(both)).toBe(8);
    // The tab's label follows its own chat's pane, the first.
    expect(taskShownIn(both, both.inFront ?? -1)).toBe(4);
  });

  it("keeps what a pane shows when another pane of the tab is split or focused", () => {
    const shown = switchTabTo(twoSessions(), 4, askedBy);
    const split = splitFocusedPane(shown, "column", 3);
    expect(onScreen(split)).toEqual([4, 3]);
    const [first] = shownIn(split, split.inFront ?? -1);
    expect(onScreen(focusPane(split, first.pane))).toEqual([4, 3]);
  });
});

describe("going back to the session's own chat", () => {
  it("shows every pane's own chat again", () => {
    const tabs = twoSessions();
    const shown = switchTabTo(tabs, 4, askedBy);
    const own = showOwn(shown, tabOf(tabs, 1));
    expect(onScreen(own)).toEqual([1]);
    expect(taskShownIn(own, tabOf(tabs, 1))).toBeUndefined();
  });

  it("answers the same tabs for a tab that shows no task", () => {
    const tabs = twoSessions();
    expect(showOwn(tabs, tabOf(tabs, 1))).toBe(tabs);
    expect(showOwn(tabs, 99)).toBe(tabs);
  });
});

describe("what a tab shows, read", () => {
  it("says each pane's own chat and the chat shown in it", () => {
    const tabs = twoSessions();
    const shown = switchTabTo(tabs, 6, askedBy);
    expect(shownIn(shown, tabOf(tabs, 1))).toEqual([
      { pane: expect.any(Number) as number, own: 1, session: 6 },
    ]);
    expect(shownIn(shown, tabOf(tabs, 2))).toEqual([
      { pane: expect.any(Number) as number, own: 2, session: 2 },
    ]);
  });

  it("draws the tab's layout with the shown chat in the session's pane", () => {
    const tabs = twoSessions();
    const shown = switchTabTo(tabs, 4, askedBy);
    const layout = layoutShown(shown, tabOf(tabs, 1));
    expect(layout).toMatchObject({ kind: "pane", content: { kind: "session", session: 4 } });
    // The tab's own layout is untouched: the session's chat is still its pane's.
    expect(shown.byId[tabOf(tabs, 1)].layout).toMatchObject({
      kind: "pane",
      content: { kind: "session", session: 1 },
    });
    // And a tab showing its own chat is drawn as it is held.
    expect(layoutShown(shown, tabOf(tabs, 2))).toBe(shown.byId[tabOf(tabs, 2)].layout);
  });

  it("says the chat that has the keyboard is the one shown", () => {
    const tabs = twoSessions();
    expect(focusedChat(tabs)).toBe(2);
    expect(focusedChat(switchTabTo(tabs, 4, askedBy))).toBe(4);
    expect(focusedChat(noTabs())).toBeUndefined();
  });
});

describe("a shown task that is gone", () => {
  it("is still what its tab shows when the core closes it: a pane never changes by itself", () => {
    const tabs = twoSessions();
    const shown = switchTabTo(tabs, 4, askedBy);

    const after = closeChat(shown, 4, oneWorkspace);

    expect(after).toBe(shown);
    expect(taskShownIn(after, tabOf(tabs, 1))).toBe(4);
  });

  it("goes back to the session's own chat in one pane, when the person leaves it", () => {
    const split = splitFocusedPane(selectTab(twoSessions(), 1), "row", 3);
    const asked = lineage({ 4: 1, 8: 3 });
    const both = switchTabTo(switchTabTo(split, 4, asked), 8, asked);
    const [first, second] = shownIn(both, both.inFront ?? -1);

    const back = showOwnIn(both, both.inFront ?? -1, first.pane);

    expect(onScreen(back)).toEqual([1, 8]);
    expect(showOwnIn(back, back.inFront ?? -1, first.pane)).toBe(back);
    expect(onScreen(showOwnIn(back, back.inFront ?? -1, second.pane))).toEqual([1, 3]);
  });

  it("falls back when it is no longer among the open chats", () => {
    const tabs = twoSessions();
    const shown = switchTabTo(switchTabTo(tabs, 4, askedBy), 7, askedBy);
    const after = keepShown(shown, (session) => session !== 4);

    expect(onScreen(selectTab(after, tabOf(tabs, 1)))).toEqual([1]);
    // The other tab's task is still open, and still shown.
    expect(onScreen(selectTab(after, tabOf(tabs, 2)))).toEqual([7]);
  });

  it("answers the same tabs when every shown task is still open", () => {
    const shown = switchTabTo(twoSessions(), 4, askedBy);
    expect(keepShown(shown, () => true)).toBe(shown);
    expect(keepShown(twoSessions(), () => false)).toEqual(twoSessions());
  });

  it("follows a shown task that was started again under a new number", () => {
    const tabs = twoSessions();
    const shown = switchTabTo(tabs, 4, askedBy);
    const after = replaceSession(shown, 4, 40);
    expect(onScreen(after)).toEqual([40]);
    expect(panesOf(after, tabOf(tabs, 1)).map((one) => one.session)).toEqual([1]);
  });

  it("is forgotten with the pane it was shown in", () => {
    const split = splitFocusedPane(selectTab(twoSessions(), 1), "row", 3);
    const shown = switchTabTo(split, 8, lineage({ 8: 3 }));
    expect(onScreen(shown)).toEqual([1, 8]);

    const closed = closeFocusedPane(shown, oneWorkspace);
    expect(onScreen(closed)).toEqual([1]);
    expect(closed.byId[closed.inFront ?? -1].shows).toBeUndefined();
  });

  it("closes the session's tab, shown task or not, when the session itself closes", () => {
    const tabs = twoSessions();
    const shown = switchTabTo(tabs, 4, askedBy);
    const after = closeChat(shown, 1, oneWorkspace);
    expect(after.order).toEqual([tabOf(tabs, 2)]);
  });
});

describe("whether a pane can draw the chat it shows", () => {
  const everyOpen: (session: number) => boolean = () => true;
  const live = (tabs: Tabs, session: number, asked: AskedBy, open = everyOpen) =>
    shownLive(tabs, tabOf(tabs, session), asked, open).map((one) => one.live);

  it("can, for its own chat, always", () => {
    expect(live(twoSessions(), 1, askedBy, () => false)).toEqual([true]);
  });

  it("can, for a task that is open and below its session", () => {
    expect(live(switchTabTo(twoSessions(), 6, askedBy), 1, askedBy)).toEqual([true]);
  });

  it("cannot, for a task that has ended", () => {
    const shown = switchTabTo(twoSessions(), 4, askedBy);
    expect(live(shown, 1, askedBy, (session) => session !== 4)).toEqual([false]);
    // And it is still what the pane shows: nothing was switched.
    expect(shownLive(shown, tabOf(shown, 1), askedBy, () => false)[0]).toMatchObject({
      own: 1,
      session: 4,
    });
  });

  it("cannot, for a task of a task whose asker has ended", () => {
    // 6 was asked for by 4, which is gone from the lineage: nothing says 6 is below 1.
    const shown = switchTabTo(twoSessions(), 6, askedBy);
    expect(live(shown, 1, lineage({ 5: 1, 7: 2 }))).toEqual([false]);
  });

  it("cannot, while the session was started again and the lineage still names the old one", () => {
    const shown = switchTabTo(twoSessions(), 4, askedBy);
    // The tabs caught up first: the session is 10 now, and the list still says 4 is 1's.
    const restarted = replaceSession(shown, 1, 10);
    expect(live(restarted, 10, askedBy)).toEqual([false]);
    // And can again once the list does too.
    expect(live(restarted, 10, lineage({ 4: 10 }))).toEqual([true]);
  });

  it("cannot, for a task that was given a tab of its own", () => {
    const shown = switchTabTo(twoSessions(), 4, askedBy);
    const both = openTab(shown, 4);
    expect(live(both, 1, askedBy)).toEqual([false]);
  });
});

describe("what the tabs showed, put back", () => {
  it("shows each task in its session's tab again, and moves nothing to the front", () => {
    const tabs = twoSessions();
    const back = restoreShown(tabs, [4, 7], askedBy);

    expect(back.inFront).toBe(tabs.inFront);
    expect(taskShownIn(back, tabOf(tabs, 1))).toBe(4);
    expect(taskShownIn(back, tabOf(tabs, 2))).toBe(7);
    // The pane that had the keyboard keeps it.
    expect(back.byId[tabOf(tabs, 1)].focused).toBe(tabs.byId[tabOf(tabs, 1)].focused);
  });

  it("leaves out a task whose session did not come back", () => {
    const tabs = twoSessions();
    const back = restoreShown(tabs, [9, 4], lineage({ 9: 8, 4: 1 }));
    expect(taskShownIn(back, tabOf(tabs, 1))).toBe(4);
    expect(taskShownIn(back, tabOf(tabs, 2))).toBeUndefined();
  });

  it("answers the same tabs when there is nothing to put back", () => {
    const tabs = twoSessions();
    expect(restoreShown(tabs, [], askedBy)).toBe(tabs);
    expect(restoreShown(tabs, [1, 2], askedBy)).toBe(tabs);
  });
});
