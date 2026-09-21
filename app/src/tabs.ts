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
 * Closes a tab. **The tab beside it IN ITS OWN WORKSPACE** comes to the front if it was the
 * one in front, and nothing is in front when its workspace held nothing else.
 *
 * The workspace is what the strip shows (ADR 0036), so a close that reached across to a tab
 * on another strip would move the operator to a workspace they did not ask for — and leave
 * the strip they are looking at with no selected tab. A plane charter has not read yet files
 * every chat the same way, and then this is the old rule exactly: the tab to the left.
 */
export function closeTab(tabs: Tabs, id: number, filedIn: FiledIn): Tabs {
  if (!(id in tabs.byId)) return tabs;
  const at = tabs.order.indexOf(id);
  const order = tabs.order.filter((tab) => tab !== id);
  const byId = Object.fromEntries(order.map((tab) => [tab, tabs.byId[tab]]));
  if (tabs.inFront !== id) return { ...tabs, byId, order };
  const workspace = workspaceOf(tabs, id, filedIn);
  const beside = order.filter((tab) => workspaceOf(tabs, tab, filedIn) === workspace);
  const before = beside.filter((tab) => tabs.order.indexOf(tab) < at);
  const inFront = before.length > 0 ? before[before.length - 1] : beside[0];
  return { ...tabs, byId, order, inFront };
}

/** Which workspace a tab belongs to: its first pane's chat is the tab's own chat. */
export function workspaceOf(tabs: Tabs, id: number, filedIn: FiledIn): string | undefined {
  const session = panesOf(tabs, id)[0]?.session;
  return session === undefined ? undefined : filedIn(session);
}

/** The tabs on one workspace's strip, left to right. */
export function tabsIn(tabs: Tabs, workspace: string | undefined, filedIn: FiledIn): number[] {
  return tabs.order.filter((id) => workspaceOf(tabs, id, filedIn) === workspace);
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
): Tabs {
  const here = tabsIn(tabs, workspace, filedIn);
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
export function closeFocusedPane(tabs: Tabs, filedIn: FiledIn): Tabs {
  const tab = frontTab(tabs);
  if (!tab) return tabs;
  const left = without(tab.layout, tab.focused);
  if (!left) return closeTab(tabs, tab.id, filedIn);
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
