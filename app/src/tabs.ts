/**
 * The tabs the window shows, and how each one is split into panes.
 *
 * **A tab is a layout of panes, and a pane holds a session or a view** (charter ADR 0043, as
 * amended 2026-09-23). A session is a chat's terminal; a view is anything else a tab can show —
 * a persona, an extension's statistics — named by data ({@link ViewRef}) rather than by a
 * component, so charter's own views and an extension's take the same path. The operator's
 * words, choosing a tab for the persona card: *"we dont have other tabs then sessions, and this
 * can be good example for us - that in tabs we can have what we want - not only harnesses"*.
 *
 * **The pane is what was generalised, not the tab.** A tab stays one thing — a layout — so a
 * view can be split beside a chat, and every rule a tab already had (the fixed order, pinning,
 * the overflow menu, closing the pane in focus) holds for a tab that shows no chat without a
 * second copy of it. What a tab does NOT have any more is a chat of its own by definition: the
 * chat's name lives on the session a pane shows, so a tab holding only a view has no chat to
 * pretend about ({@link chatOf}).
 *
 * Only the tab in front has panes on screen, which is what keeps fifty sessions cheap: the core
 * holds every session's terminal, and the panes that are visible are the only ones drawing.
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

/**
 * A view, named by data: **who draws it, which of theirs, and what it is about.**
 *
 * `from` is `null` for a view charter draws itself and an approved extension's id for one it
 * offers, which is `charter_core::panel::By` on the wire; `view` is which of theirs; `key` is
 * what it is about inside that — a persona's name, or `""` for the whole plane. The persona view
 * is `{ from: null, view: "persona", key: "steward" }` and persona statistics is
 * `{ from: "persona-statistics", view: "statistics", key: "" }`: **the same shape, the same
 * command (`open_view`), the same renderer** — nothing about a built-in view is a path an
 * extension's cannot take.
 */
export type ViewRef = { from: string | null; view: string; key: string };

/** What a pane shows. */
export type Content =
  /**
   * A chat's terminal. `chat` is the chat's own name, as the plane records it and as the core
   * was told it — separate from the tab's `name`, because a split starts a second chat under
   * the first one's name and the core must be told that name, not the sentence the tab draws.
   */
  | { kind: "session"; session: number; chat: string }
  /**
   * A view. **It carries its workspace, because it has nothing else to be filed by**: a chat is
   * on the strip of the workspace it works in (the plane's answer, `FiledIn`), and a view works
   * nowhere. So it is on the strip it was opened from — the one in front — and says so.
   */
  | {
      kind: "view";
      view: ViewRef;
      workspace: string;
      /**
       * Put back by a launch, and **not asked anything until the operator presses for it.**
       *
       * An extension's view runs that extension's program when it is asked (ADR 0041 stage 2,
       * *one round trip per deliberate human action*), and a tab the record put back was
       * opened by nobody at this launch — the record is a file in the plane, and a line in it
       * must not become a program run at every start. So a put-back tab waits, says so, and
       * asks on a press; one the operator opens is asked at once. charter's own views run no
       * program and ignore it.
       */
      waits?: boolean;
    };

export type Layout =
  | { kind: "pane"; pane: number; content: Content }
  | { kind: "split"; direction: Direction; children: [Layout, Layout] };

export type Tab = {
  id: number;
  /**
   * What the tab bar shows: the chat's name and the persona it adopted, where charter knows
   * one — `3 steward` rather than `3` (charter-app#130) — or the view's title for a tab that
   * opened on a view.
   *
   * A number identifies a chat to charter and tells the operator nothing, and the strip is
   * where an operator with fifty of them works out which is which. The persona is known at
   * the moment a tab opens on both paths — the picker carries the operator's choice, and a
   * chat put back at a launch carries its own — so this is never filled in later.
   */
  name: string;
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
  const named = chat || String(tabs.named.tabs + 1);
  return withTab(
    tabs,
    persona ? `${named} ${persona}` : named,
    { kind: "session", session, chat: named },
    tabs.order.length,
  );
}

/** The one place a tab is minted: `content` in one pane, at `at` in the order, in front. */
function withTab(tabs: Tabs, name: string, content: Content, at: number): Tabs {
  const id = tabs.named.tabs + 1;
  const pane = tabs.named.panes + 1;
  const order = [...tabs.order];
  order.splice(Math.max(0, Math.min(at, order.length)), 0, id);
  return {
    byId: {
      ...tabs.byId,
      [id]: { id, name, layout: { kind: "pane", pane, content }, focused: pane },
    },
    order,
    inFront: id,
    named: { tabs: id, panes: pane },
  };
}

/** What one view is called as a key: unique per plane, and the same for the same view. */
export function viewKey(view: ViewRef): string {
  // Namespaced as `charter_core::panel::Panel::key` is, so an extension that calls itself
  // `charter` cannot answer to charter's own view's name.
  return view.from === null
    ? `charter/${view.view}/${view.key}`
    : `ext/${view.from}/${view.view}/${view.key}`;
}

/** The tab and pane already showing `view`, where one is. */
export function findView(tabs: Tabs, view: ViewRef): { tab: number; pane: number } | undefined {
  const wanted = viewKey(view);
  for (const tab of tabs.order) {
    const found = contents(tabs.byId[tab].layout).find(
      (one) => one.content.kind === "view" && viewKey(one.content.view) === wanted,
    );
    if (found) return { tab, pane: found.pane };
  }
  return undefined;
}

/**
 * Opens `view` in a tab of its own, in front — **or brings forward the one already showing it.**
 *
 * One view, one surface: a second tab for the persona that already has one is two answers to
 * "where is steward" that can drift apart (one scrolled, one searched), and the operator asked
 * for a tab, not a tab per click. The pane showing it takes the focus too, so a view that is
 * one side of a split is the side that answers the keyboard.
 *
 * `workspace` is the strip it goes on: the one in front, which is where it was opened from.
 */
export function openView(tabs: Tabs, view: ViewRef, name: string, workspace: string): Tabs {
  const open = findView(tabs, view);
  if (open) {
    const tab = tabs.byId[open.tab];
    // Opening it is the operator asking for it, so a tab that was waiting for a press has had
    // one (`Content.waits`).
    const layout = replace(tab.layout, open.pane, (found) =>
      found.content.kind === "view" && found.content.waits
        ? { ...found, content: { ...found.content, waits: false } }
        : found,
    );
    return {
      ...tabs,
      byId: { ...tabs.byId, [tab.id]: { ...tab, layout, focused: open.pane } },
      inFront: tab.id,
    };
  }
  return withTab(tabs, name, { kind: "view", view, workspace }, tabs.order.length);
}

/**
 * Puts back a view tab the last launch recorded, at the place it had — **behind** whatever is in
 * front, because a launch decides what is in front once, from the whole record.
 *
 * `at` is where it was on the strip, over chats and views together. It is a place and not a
 * promise: a chat that did not come back moves it by one, and a place past the end is the end.
 * A view already open is left where it is rather than drawn twice.
 */
export function putViewBack(
  tabs: Tabs,
  view: ViewRef,
  name: string,
  workspace: string,
  at: number,
): Tabs {
  if (findView(tabs, view)) return tabs;
  const opened = withTab(tabs, name, { kind: "view", view, workspace, waits: true }, at);
  return { ...opened, inFront: tabs.inFront };
}

/**
 * The operator pressed to have a waiting view asked (`Content.waits`): that pane of the tab in
 * front stops waiting. Anything else is left as it is.
 */
export function stopWaiting(tabs: Tabs, pane: number): Tabs {
  const tab = frontTab(tabs);
  if (!tab) return tabs;
  let changed = false;
  const layout = replace(tab.layout, pane, (found) => {
    if (found.content.kind !== "view" || !found.content.waits) return found;
    changed = true;
    return { ...found, content: { ...found.content, waits: false } };
  });
  return changed ? { ...tabs, byId: { ...tabs.byId, [tab.id]: { ...tab, layout } } } : tabs;
}

/**
 * The tab's own chat: **the session in its first pane, and nothing when that pane is a view.**
 *
 * What a tab's state mark, its pin and the core's "chat in front" are about. A tab that opened
 * on a view and was split to start a chat beside it is still the view's tab — its first pane
 * says what it is — so it draws no chat state and has no chat to pin.
 */
export function chatOf(tabs: Tabs, id: number): number | undefined {
  const first = contentsOf(tabs, id)[0]?.content;
  return first?.kind === "session" ? first.session : undefined;
}

/**
 * The name a new chat started beside this tab's panes is given: the name of the first chat in
 * it, or nothing when it has none — a view's tab split to start a chat starts one with a name of
 * its own, not the view's title.
 */
export function chatNameOf(tabs: Tabs, id: number): string | undefined {
  const tab = tabs.byId[id];
  if (!tab) return undefined;
  const first = contents(tab.layout).find((one) => one.content.kind === "session");
  return first?.content.kind === "session" ? first.content.chat : undefined;
}

/** What each pane of a tab shows, left to right and top to bottom. */
export function contentsOf(tabs: Tabs, id: number): { pane: number; content: Content }[] {
  const tab = tabs.byId[id];
  return tab ? contents(tab.layout) : [];
}

/** What the focused pane of the tab in front shows, when anything is in front. */
export function focusedContent(tabs: Tabs): Content | undefined {
  const tab = frontTab(tabs);
  return tab && contents(tab.layout).find((one) => one.pane === tab.focused)?.content;
}

/**
 * Every view tab whose strip is not a workspace any more, moved to `outside`.
 *
 * A chat's strip is re-read off the plane every time (`FiledIn`), so a workspace that goes takes
 * its chats' strip with it. A view carries its own, so nothing would move it: it would be on a
 * strip that is never drawn, unreachable. This is the one place that says what a view's strip is
 * when the plane stops having it — the same strip a chat that works in no workspace is on.
 *
 * Answers `tabs` itself when nothing moved, so a caller can tell.
 */
export function refileViews(
  tabs: Tabs,
  stands: (workspace: string) => boolean,
  outside: string,
): Tabs {
  let moved = false;
  const refile = (layout: Layout): Layout => {
    if (layout.kind === "split")
      return { ...layout, children: layout.children.map(refile) as [Layout, Layout] };
    const content = layout.content;
    if (content.kind !== "view" || content.workspace === outside || stands(content.workspace))
      return layout;
    moved = true;
    return { ...layout, content: { ...content, workspace: outside } };
  };
  const byId = Object.fromEntries(
    tabs.order.map((id) => [id, { ...tabs.byId[id], layout: refile(tabs.byId[id].layout) }]),
  );
  return moved ? { ...tabs, byId } : tabs;
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

/**
 * Which workspace a tab belongs to: **its first pane's.** A chat is filed where it works — the
 * plane's answer — and a view where it was opened, which it carries.
 */
export function workspaceOf(tabs: Tabs, id: number, filedIn: FiledIn): string | undefined {
  const first = contentsOf(tabs, id)[0]?.content;
  if (first === undefined) return undefined;
  return first.kind === "session" ? filedIn(first.session) : first.workspace;
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

/**
 * Divides the focused pane in two, the new one showing `session` and focused.
 *
 * `chat` is the new chat's name. It defaults to the name of the tab's own chat, which is what a
 * split has always meant; a tab showing only a view has none, and the caller names it.
 */
export function splitFocusedPane(
  tabs: Tabs,
  direction: Direction,
  session: number,
  chat?: string,
): Tabs {
  const tab = frontTab(tabs);
  if (!tab) return tabs;
  const pane = tabs.named.panes + 1;
  const content: Content = {
    kind: "session",
    session,
    chat: chat ?? chatNameOf(tabs, tab.id) ?? String(session),
  };
  const layout = replace(tab.layout, tab.focused, (focused) => ({
    kind: "split",
    direction,
    children: [focused, { kind: "pane", pane, content }],
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

/**
 * The panes of a tab **that show a chat**, left to right and top to bottom.
 *
 * Every caller of this is asking about chats — which to end when the tab closes, which one a
 * queue row brings forward, when the tab last moved — and a pane showing a view has none of
 * those. {@link contentsOf} is every pane.
 */
export function panesOf(tabs: Tabs, id: number): { pane: number; session: number }[] {
  return contentsOf(tabs, id).flatMap(({ pane, content }) =>
    content.kind === "session" ? [{ pane, session: content.session }] : [],
  );
}

/** The sessions with a pane on screen: the only ones a terminal is drawing. */
export function visibleSessions(tabs: Tabs): number[] {
  return tabs.inFront === undefined ? [] : panesOf(tabs, tabs.inFront).map((one) => one.session);
}

function frontTab(tabs: Tabs): Tab | undefined {
  return tabs.inFront === undefined ? undefined : tabs.byId[tabs.inFront];
}

type Pane = Extract<Layout, { kind: "pane" }>;

function panes(layout: Layout): Pane[] {
  return layout.kind === "pane" ? [layout] : layout.children.flatMap(panes);
}

function contents(layout: Layout): { pane: number; content: Content }[] {
  return panes(layout).map(({ pane, content }) => ({ pane, content }));
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
