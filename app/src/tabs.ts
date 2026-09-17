/**
 * The tabs the window shows, and how each one is split into panes.
 *
 * A pane shows one session. Only the tab in front has panes on screen, which is what keeps
 * fifty sessions cheap: the core holds every session's terminal, and the panes that are
 * visible are the only ones drawing.
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
  layout: Layout;
  /** The pane a split or a close acts on. */
  focused: number;
};

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
export function openTab(tabs: Tabs, session: number): Tabs {
  const id = tabs.named.tabs + 1;
  const pane = tabs.named.panes + 1;
  return {
    byId: { ...tabs.byId, [id]: { id, layout: { kind: "pane", pane, session }, focused: pane } },
    order: [...tabs.order, id],
    inFront: id,
    named: { tabs: id, panes: pane },
  };
}

/** Closes a tab. The tab beside it comes to the front if it was the one in front. */
export function closeTab(tabs: Tabs, id: number): Tabs {
  if (!(id in tabs.byId)) return tabs;
  const at = tabs.order.indexOf(id);
  const order = tabs.order.filter((tab) => tab !== id);
  const byId = Object.fromEntries(order.map((tab) => [tab, tabs.byId[tab]]));
  const inFront =
    tabs.inFront === id ? (order[Math.max(at - 1, 0)] as number | undefined) : tabs.inFront;
  return { ...tabs, byId, order, inFront };
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
export function closeFocusedPane(tabs: Tabs): Tabs {
  const tab = frontTab(tabs);
  if (!tab) return tabs;
  const left = without(tab.layout, tab.focused);
  if (!left) return closeTab(tabs, tab.id);
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
