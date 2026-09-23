/**
 * The tabs the window shows, and how each one is split into panes.
 *
 * A pane shows one session. Only the tab in front has panes on screen, which is what keeps
 * fifty sessions cheap: the core holds every session's terminal, and the panes that are
 * visible are the only ones drawing.
 *
 * **A tab belongs to a workspace, and the strip shows one workspace's tabs** (ADR 0036). It is
 * not a field on the tab: which workspace a chat is in is the plane's answer, read off the
 * sidebar, so the functions that need it take a `FiledIn` and ask.
 *
 * **`order` is fixed, and that is now a decision rather than an accident** (ADR 0039). A tab
 * is appended when it opens, taken out when it closes, and nothing else ever touches its
 * place. A tab that moves under the cursor breaks aiming: an operator going back to the chat
 * that was third from the left goes there with their hand, not by reading, and a strip that
 * re-sorted on activity would turn every click into a read. This is what browsers do, and it
 * is the one interaction convention in this window every user already has.
 *
 * The one place activity DOES order anything is the overflow menu — `byLastActivity` below,
 * and nowhere else. The rule and its opposite are the same rule from two sides: the menu is a
 * list you read, the strip is a surface you aim at, and the boundary is whether the thing
 * moves under your hand.
 *
 * Everything here is a plain value, so the window's whole arrangement is one state to test.
 */

/** Which way a split divides its two children: `row` side by side, `column` one above the other. */
export type Direction = "row" | "column";

export type Layout =
  | { kind: "pane"; pane: number; session: number }
  | { kind: "split"; direction: Direction; children: [Layout, Layout] };

export type Tab = {
  id: number;
  /**
   * What the tab bar shows: the chat's name and the persona it adopted, where charter knows
   * one — `3 steward` rather than `3` (charter-app#130).
   *
   * A number identifies a chat to charter and tells the operator nothing, and the strip is
   * where an operator with fifty of them works out which is which. The persona is known at
   * the moment a tab opens on both paths — the picker carries the operator's choice, and a
   * chat put back at a launch carries its own — so this is never filled in later.
   */
  name: string;
  /**
   * The chat's own name, as the plane records it and as the core was told it.
   *
   * Separate from `name`, because a split starts a second chat under the tab's name and the
   * core must be told the chat's name, not the sentence the tab bar draws.
   */
  chat: string;
  layout: Layout;
  /** The pane a split or a close acts on. */
  focused: number;
};

/**
 * Which workspace a chat is filed under — the strip its tab appears on.
 *
 * A function rather than a field on the tab, because **the plane is what files a chat**: what
 * relates a chat to a workspace is the directory it works in, and the sidebar the core reads
 * off the plane is the answer to that. A copy on the tab would be a second answer that
 * nothing invalidates when the plane changes under it.
 */
export type FiledIn = (session: number) => string;

export type Tabs = {
  byId: Record<number, Tab>;
  /** Left to right, as the tab bar shows them. */
  order: number[];
  /** The tab on screen, or none when every tab has closed. */
  inFront?: number;
  /** Ids already handed out, so a new tab or pane never reuses one. */
  named: { tabs: number; panes: number };
};

export function noTabs(): Tabs {
  return { byId: {}, order: [], named: { tabs: 0, panes: 0 } };
}

/** Opens a tab with one pane showing `session`, in front. */
export function openTab(
  tabs: Tabs,
  session: number,
  chat = "",
  persona: string | null = null,
): Tabs {
  const id = tabs.named.tabs + 1;
  const pane = tabs.named.panes + 1;
  const named = chat || String(id);
  return {
    byId: {
      ...tabs.byId,
      [id]: {
        id,
        name: persona ? `${named} ${persona}` : named,
        chat: named,
        layout: { kind: "pane", pane, session },
        focused: pane,
      },
    },
    order: [...tabs.order, id],
    inFront: id,
    named: { tabs: id, panes: pane },
  };
}

/**
 * Opens a tab for a chat the operator did not open from this window: one a handoff opened
 * (charter-app#204). **It does not take the front.**
 *
 * A handoff is work the operator sent away from the chat they are reading, and the chat it
 * opened is often on another workspace's strip. Taking the front would interrupt the chat on
 * screen or move the window to a workspace nobody asked to look at. The tab is on its strip,
 * and that is the whole of how it is seen.
 *
 * The one exception is a window with nothing in front: there is nothing to interrupt, and a
 * tab on a strip with nothing in front of it is a blank pane.
 */
export function openTabBehind(
  tabs: Tabs,
  session: number,
  chat = "",
  persona: string | null = null,
): Tabs {
  const opened = openTab(tabs, session, chat, persona);
  return tabs.inFront === undefined ? opened : { ...opened, inFront: tabs.inFront };
}

/**
 * Closes a tab. **The tab beside it IN ITS OWN WORKSPACE** comes to the front if it was the
 * one in front, and nothing is in front when its workspace held nothing else.
 *
 * The workspace is what the strip shows (ADR 0036), so a close that reached across to a tab
 * on another strip would move the operator to a workspace they did not ask for — and leave
 * the strip they are looking at with no selected tab. A plane charter has not read yet files
 * every chat the same way, and then this is the old rule exactly: the tab to the left.
 *
 * **"Beside" means beside ON THE STRIP**, which is why this takes `pinned` too: a pin draws a
 * tab first, and an operator closing the third tab means the tab they can see to its left,
 * not the one that happens to be before it in the order the chats were opened.
 */
export function closeTab(
  tabs: Tabs,
  id: number,
  filedIn: FiledIn,
  pinned: Pinned = nothingPinned,
): Tabs {
  if (!(id in tabs.byId)) return tabs;
  const workspace = workspaceOf(tabs, id, filedIn);
  const strip = tabsIn(tabs, workspace, filedIn, pinned);
  const at = strip.indexOf(id);
  const order = tabs.order.filter((tab) => tab !== id);
  const byId = Object.fromEntries(order.map((tab) => [tab, tabs.byId[tab]]));
  if (tabs.inFront !== id) return { ...tabs, byId, order };
  const beside = strip.filter((tab) => tab !== id);
  const inFront = at > 0 ? strip[at - 1] : beside[0];
  return { ...tabs, byId, order, inFront };
}

/** Which workspace a tab belongs to: its first pane's chat is the tab's own chat. */
export function workspaceOf(tabs: Tabs, id: number, filedIn: FiledIn): string | undefined {
  const session = panesOf(tabs, id)[0]?.session;
  return session === undefined ? undefined : filedIn(session);
}

/**
 * Whether a tab is pinned, which is a fact about the operator and not about the chat.
 *
 * A function rather than a field, for the same reasons `FiledIn` and `LastMoved` are: it is
 * answered somewhere else — the plane's own app record, through the core (ADR 0040) — and a
 * copy on the tab would be a second answer nothing invalidates.
 */
export type Pinned = (id: number) => boolean;

/** Nothing pinned, which is what every caller that does not care about pins passes. */
export const nothingPinned: Pinned = () => false;

/**
 * The tabs on one workspace's strip, left to right — **pinned ones first** (ADR 0039).
 *
 * **This is not the strip re-ordering itself.** The rule ADR 0039 fixed is that a tab does not
 * MOVE under the cursor: an operator going back to the chat that was third from the left goes
 * there with their hand, and a strip that re-sorted on activity would turn every click into a
 * read. A pin is the opposite of that — it is the operator putting a tab where they want it,
 * once, deliberately, and it is what pinning means in every browser that has it. Nothing
 * moves that the operator did not move.
 *
 * Within each group the order is `tabs.order`'s, which is the order the chats were opened and
 * is never touched. Pinning two tabs does not sort them against each other.
 *
 * **And this is the whole of what a pin does to the overflow**, which ADR 0039 left open: a
 * pinned tab is first, so it is the last thing the strip scrolls away, and it needs no
 * exemption of its own. An exemption — a tab held out of the scroller — would be a second
 * mechanism deciding what is on screen, and the one thing the strip owes is that nothing in
 * it is unreachable.
 */
export function tabsIn(
  tabs: Tabs,
  workspace: string | undefined,
  filedIn: FiledIn,
  pinned: Pinned = nothingPinned,
): number[] {
  const here = tabs.order.filter((id) => workspaceOf(tabs, id, filedIn) === workspace);
  return [...here.filter(pinned), ...here.filter((id) => !pinned(id))];
}

/**
 * When a chat last moved, as the core counts moves on its plane. Bigger is more recent.
 *
 * A function rather than a field, for the same reason `FiledIn` is: the count is the core's,
 * it arrives on `chat-moved`, and a copy on the tab would be a second answer that nothing
 * invalidates when the next event lands.
 */
export type LastMoved = (session: number) => number;

/** When a TAB last moved: the most recent of the chats in its panes. */
export function movedAt(tabs: Tabs, id: number, lastMoved: LastMoved): number {
  return panesOf(tabs, id).reduce((most, pane) => Math.max(most, lastMoved(pane.session)), 0);
}

/**
 * `ids` most recently moved first — the order the overflow menu lists them in (ADR 0039).
 *
 * **Ties keep the order they came in**, which is the strip's, because `Array.sort` is stable
 * in every engine this app runs on. That matters more than it looks: a chat nothing has been
 * heard about reads `0`, so at a launch every tab ties and the menu is the strip's order
 * rather than a shuffle. A menu whose rows moved between two openings for no reason the
 * operator can see is the aiming defect ADR 0039 refuses, re-introduced in the one surface
 * that was allowed to sort.
 *
 * It answers a new array and never touches `tabs.order`. The strip's order is fixed.
 */
export function byLastActivity(ids: readonly number[], tabs: Tabs, lastMoved: LastMoved): number[] {
  return [...ids].sort(
    (one, other) => movedAt(tabs, other, lastMoved) - movedAt(tabs, one, lastMoved),
  );
}

/**
 * The arrangement after a workspace is focused: one of ITS tabs in front, and none at all
 * when it holds none.
 *
 * `prefer` is the tab that was in front there last, which is what an operator coming back to
 * a workspace means by it. A workspace with nothing in it puts nothing in front rather than
 * leaving another workspace's chat on screen under this workspace's empty strip.
 */
export function showWorkspace(
  tabs: Tabs,
  workspace: string | undefined,
  filedIn: FiledIn,
  prefer?: number,
  pinned: Pinned = nothingPinned,
): Tabs {
  // The strip's order, so "its first" is the tab the operator can see first — which is a
  // pinned one where there is one.
  const here = tabsIn(tabs, workspace, filedIn, pinned);
  if (here.length === 0) return { ...tabs, inFront: undefined };
  return { ...tabs, inFront: prefer !== undefined && here.includes(prefer) ? prefer : here[0] };
}

export function selectTab(tabs: Tabs, id: number): Tabs {
  return id in tabs.byId ? { ...tabs, inFront: id } : tabs;
}

/** Focuses a pane of the tab in front, so the next split or close acts on it. */
export function focusPane(tabs: Tabs, pane: number): Tabs {
  const tab = frontTab(tabs);
  if (!tab || !panes(tab.layout).some((each) => each.pane === pane)) return tabs;
  return { ...tabs, byId: { ...tabs.byId, [tab.id]: { ...tab, focused: pane } } };
}

/** Divides the focused pane in two, the new one showing `session` and focused. */
export function splitFocusedPane(tabs: Tabs, direction: Direction, session: number): Tabs {
  const tab = frontTab(tabs);
  if (!tab) return tabs;
  const pane = tabs.named.panes + 1;
  const layout = replace(tab.layout, tab.focused, (focused) => ({
    kind: "split",
    direction,
    children: [focused, { kind: "pane", pane, session }],
  }));
  return {
    ...tabs,
    byId: { ...tabs.byId, [tab.id]: { ...tab, layout, focused: pane } },
    named: { ...tabs.named, panes: pane },
  };
}

/**
 * Closes the focused pane. What shared its split takes the split's place; when it was the
 * tab's only pane, the tab closes with it.
 */
export function closeFocusedPane(
  tabs: Tabs,
  filedIn: FiledIn,
  pinned: Pinned = nothingPinned,
): Tabs {
  const tab = frontTab(tabs);
  if (!tab) return tabs;
  const left = without(tab.layout, tab.focused);
  if (!left) return closeTab(tabs, tab.id, filedIn, pinned);
  const focused = panes(left)[0].pane;
  return { ...tabs, byId: { ...tabs.byId, [tab.id]: { ...tab, layout: left, focused } } };
}

/** The panes of a tab, left to right and top to bottom. */
export function panesOf(tabs: Tabs, id: number): { pane: number; session: number }[] {
  const tab = tabs.byId[id];
  return tab ? panes(tab.layout).map(({ pane, session }) => ({ pane, session })) : [];
}

/** The sessions with a pane on screen: the only ones a terminal is drawing. */
export function visibleSessions(tabs: Tabs): number[] {
  const tab = frontTab(tabs);
  return tab ? panes(tab.layout).map((pane) => pane.session) : [];
}

function frontTab(tabs: Tabs): Tab | undefined {
  return tabs.inFront === undefined ? undefined : tabs.byId[tabs.inFront];
}

type Pane = Extract<Layout, { kind: "pane" }>;

function panes(layout: Layout): Pane[] {
  return layout.kind === "pane" ? [layout] : layout.children.flatMap(panes);
}

/** `layout` with the pane `pane` put through `change`. */
function replace(layout: Layout, pane: number, change: (found: Pane) => Layout): Layout {
  if (layout.kind === "pane") return layout.pane === pane ? change(layout) : layout;
  return {
    ...layout,
    children: layout.children.map((child) => replace(child, pane, change)) as [Layout, Layout],
  };
}

/** `layout` without the pane `pane`, or nothing when it was the only one. */
function without(layout: Layout, pane: number): Layout | undefined {
  if (layout.kind === "pane") return layout.pane === pane ? undefined : layout;
  const [first, second] = layout.children.map((child) => without(child, pane));
  if (!first) return second;
  if (!second) return first;
  return { ...layout, children: [first, second] };
}
