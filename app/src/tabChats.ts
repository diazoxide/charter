/**
 * **The chats a session's tab can show** (#1486): the session's own chat and the tasks below
 * it, read from the two things that already exist. The tabs say which chat each pane is and
 * what it shows (`tabs.ts`); the core's list says who asked whom (`chatsTree.ts`). Nothing
 * here is state: every answer is derived, so the Chats list, a tab's label, a pane's
 * breadcrumb and a tab's menu cannot disagree about which chats are a tab's.
 */
import { chatsTree, type ChatRow, type ListedChat } from "./chatsTree";
import {
  homeOf,
  panesOf,
  placedOf,
  sessionOf,
  shownIn,
  type AskedBy,
  type Placed,
  type Tabs,
} from "./tabs";

/** Who asked whom, as the tabs are asked it (`tabs.AskedBy`), read off the core's list. */
export function askedByOf(chats: readonly ListedChat[]): AskedBy {
  // **Task links only** (V100-69): a handoff moved the work and is a session of its own, so
  // it is never shown inside the tab of the chat it came from, with or without a tab here.
  const parents = new Map(
    chats.map((chat) => [chat.session, chat.mode === "task" ? chat.parent : null]),
  );
  return (session) => parents.get(session) ?? undefined;
}

/**
 * **The chats of tab `id`, as a tree**: each of its panes' own chats, then every chat below it
 * whose home is that pane (`tabs.homeOf`), nested by who asked.
 *
 * **A task with a pane of its own is still its session's** (#1489, V100-38): it is listed
 * under the chat that asked for it, marked with where it is (`ChatRow.placed`). One in a tab
 * of its own is listed alone, since what is below it is that tab's; one beside its session, in
 * this tab, is listed with what is below it, and its pane is not listed a second time.
 *
 * The one selector for "which chats does this tab hold": a tab's menu lists these rows, and
 * its count of working and finished tasks is a count over them.
 */
export function chatsOfTab(tabs: Tabs, id: number, chats: readonly ListedChat[]): ChatRow[] {
  return chatsOfTabs(tabs, chats).get(id) ?? [];
}

/** The chats of one pane of tab `id`: the pane's own chat first, then the tasks at home in
 *  it (`tabs.homeOf`). **Strictly the pane's**: a task with a pane of its own is that pane's,
 *  and its Notices are drawn there. */
export function chatsOfPane(
  tabs: Tabs,
  id: number,
  pane: number,
  chats: readonly ListedChat[],
): ChatRow[] {
  const askedBy = askedByOf(chats);
  const here = chats.filter((chat) => {
    const home = homeOf(tabs, chat.session, askedBy);
    return home !== undefined && home.tab === id && home.pane === pane;
  });
  // Nested among themselves: the pane's own chat is the only one whose asker is not here, so
  // it is never an orphan in this tree, whatever started it.
  return chatsTree(here).map((row) => ({ ...row, orphaned: false }));
}

/**
 * **The chats of every tab, each as {@link chatsOfTab} lists its own**, by tab (#1487): what
 * the strip draws each tab's chip from. One walk of the list for all of them, since a strip
 * of fifty tabs asking {@link chatsOfTab} fifty times reads every chat fifty times.
 */
export function chatsOfTabs(
  tabs: Tabs,
  chats: readonly ListedChat[],
): ReadonlyMap<number, ChatRow[]> {
  const askedBy = askedByOf(chats);
  const key = (at: { tab: number; pane: number }) => `${at.tab}:${at.pane}`;
  /**
   * The pane a chat is listed under: its home, and for a task beside its session, in the
   * session's own tab, the session's pane, however many such steps there are.
   */
  const listedAt = (home: { tab: number; pane: number; own: number }) => {
    let at = home;
    const seen = new Set<number>();
    while (!seen.has(at.own)) {
      seen.add(at.own);
      const session = sessionOf(tabs, at.own, askedBy);
      if (session === undefined || session.tab !== at.tab) break;
      at = session;
    }
    return at;
  };
  const here = new Map<string, ListedChat[]>();
  /** Where each task with a pane of its own is, by the pane it is listed under. */
  const placed = new Map<string, Map<number, Placed>>();
  const add = (under: string, chat: ListedChat) => {
    const among = here.get(under);
    if (among === undefined) here.set(under, [chat]);
    else among.push(chat);
  };
  /** The panes whose own chat is listed under another pane of the same tab. */
  const folded = new Set<string>();
  for (const chat of chats) {
    const home = homeOf(tabs, chat.session, askedBy);
    if (home === undefined) continue;
    const under = listedAt(home);
    add(key(under), chat);
    if (home.own !== chat.session) continue;
    const where = placedOf(tabs, chat.session, askedBy);
    if (where === undefined) continue;
    if (under.pane !== home.pane || under.tab !== home.tab) {
      // Beside its session, in this tab: under it, with everything below.
      folded.add(key(home));
      const marks = placed.get(key(under)) ?? new Map<number, Placed>();
      placed.set(key(under), marks.set(chat.session, "beside"));
      continue;
    }
    // In another tab than its session's: listed there too, alone.
    const session = sessionOf(tabs, chat.session, askedBy);
    if (session === undefined) continue;
    const there = key(listedAt(session));
    add(there, chat);
    const marks = placed.get(there) ?? new Map<number, Placed>();
    placed.set(there, marks.set(chat.session, where));
  }
  return new Map(
    tabs.order.map((id) => [
      id,
      panesOf(tabs, id).flatMap(({ pane }) => {
        const at = `${id}:${pane}`;
        if (folded.has(at)) return [];
        const marks = placed.get(at);
        return chatsTree(here.get(at) ?? []).map((row): ChatRow => {
          const where = marks?.get(row.session);
          return where === undefined
            ? { ...row, orphaned: false }
            : { ...row, orphaned: false, placed: where };
        });
      }),
    ]),
  );
}

/** What a pane's breadcrumb says while the pane shows a task. */
export type Crumbs = {
  /** The session's own chat first, then each chat that asked, then the chat shown. */
  path: ListedChat[];
  /** The workspace the shown chat works in, when it is not its session's. */
  elsewhere: string | null;
};

/**
 * **The breadcrumb of each pane of tab `id` that shows a task**, by pane: the path from the
 * session's own chat down to the chat shown. A pane showing its own chat has none, and so has
 * a shown chat the core's list does not hold: nothing is said of a chat nothing is known of.
 */
export function crumbsOf(
  tabs: Tabs,
  id: number,
  chats: readonly ListedChat[],
): Record<number, Crumbs> {
  const byNumber = new Map(chats.map((chat) => [chat.session, chat]));
  const crumbs: Record<number, Crumbs> = {};
  for (const { pane, own, session } of shownIn(tabs, id)) {
    if (session === own) continue;
    const path: ListedChat[] = [];
    const seen = new Set<number>();
    let at = byNumber.get(session);
    while (at !== undefined && !seen.has(at.session)) {
      seen.add(at.session);
      path.unshift(at);
      if (at.session === own) break;
      at = at.parent === null || at.mode !== "task" ? undefined : byNumber.get(at.parent);
    }
    const [top] = path;
    const shown = path[path.length - 1];
    // The path must reach the session: a chat whose asker is not listed has no path to say.
    if (top === undefined || top.session !== own || shown.session !== session) continue;
    crumbs[pane] = {
      path,
      elsewhere: shown.workspace === top.workspace ? null : shown.workspace,
    };
  }
  return crumbs;
}

/**
 * **The breadcrumb of each pane of tab `id` that shows its own chat and still has a path to
 * say** (#1489), by pane. A task in a pane of its own says the path down to it, from the
 * session at the top, as a pane switched to it would: it is why a task's own tab is not
 * mistaken for a session's. And where a task is beside a session, in one tab, the session's
 * pane says its own name, so each side of the split says which chat it is.
 *
 * A session with no task beside it has none, and its top line is the one it had. Neither has
 * a task whose asker is not listed any more: its own name is no path, and its tab says it.
 */
export function placedCrumbsOf(
  tabs: Tabs,
  id: number,
  chats: readonly ListedChat[],
): Record<number, Crumbs> {
  const byNumber = new Map(chats.map((chat) => [chat.session, chat]));
  const crumbs: Record<number, Crumbs> = {};
  const shown = shownIn(tabs, id);
  const isTask = (session: number) => byNumber.get(session)?.mode === "task";
  const split = shown.some((one) => isTask(one.own));
  for (const { pane, own, session } of shown) {
    if (session !== own) continue;
    const chat = byNumber.get(own);
    if (chat === undefined) continue;
    if (chat.mode !== "task") {
      if (split && shown.length > 1) crumbs[pane] = { path: [chat], elsewhere: null };
      continue;
    }
    const path: ListedChat[] = [];
    const seen = new Set<number>();
    let at: ListedChat | undefined = chat;
    while (at !== undefined && !seen.has(at.session)) {
      seen.add(at.session);
      path.unshift(at);
      at = at.parent === null || at.mode !== "task" ? undefined : byNumber.get(at.parent);
    }
    // Its own name alone is no path: the tab already says it.
    if (path.length < 2) continue;
    crumbs[pane] = {
      path,
      elsewhere: chat.workspace === path[0].workspace ? null : chat.workspace,
    };
  }
  return crumbs;
}

/**
 * **The chats of tab `id` that are waiting for the person and are not on screen**, in the
 * order they wait: what the tab wears the hand for (V100-37).
 *
 * A chat is on screen only when its pane shows it AND its tab is in front: a task the person
 * left a tab on, and then went elsewhere, is not on screen when it asks. The session's own
 * chat counts when its pane shows another chat. While its pane shows it, the tab's own state
 * mark is that chat's, in front or behind, as it is for a session with no tasks.
 *
 * **A task moved to a tab of its own still asks through the session that asked it** (#1601):
 * while its own tab is not in front, it and its own tasks are counted for the tab of the chat
 * that asked it too, so the session's chip wears the hand and names it. A handoff is no task
 * (`askedByOf`), so it never is.
 */
export function hiddenNeeding(
  tabs: Tabs,
  id: number,
  askedBy: AskedBy,
  /** The chats waiting for the person, the longest waiting first. */
  waiting: readonly number[],
): number[] {
  if (waiting.length === 0) return [];
  const front = tabs.inFront === id;
  const shown = new Map(shownIn(tabs, id).map((one) => [one.pane, one.session]));
  return waiting.filter((session) => {
    const home = homeOf(tabs, session, askedBy);
    if (home === undefined) return false;
    if (home.tab !== id) {
      if (tabs.inFront === home.tab) return false;
      const asker = askedBy(home.own);
      return asker !== undefined && homeOf(tabs, asker, askedBy)?.tab === id;
    }
    const onPane = shown.get(home.pane) === session;
    return home.own === session ? !onPane : !(onPane && front);
  });
}
