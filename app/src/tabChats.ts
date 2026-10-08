/**
 * **The chats a session's tab can show** (#1486): the session's own chat and the tasks below
 * it, read from the two things that already exist. The tabs say which chat each pane is and
 * what it shows (`tabs.ts`); the core's list says who asked whom (`chatsTree.ts`). Nothing
 * here is state: every answer is derived, so the Chats list, a tab's label, a pane's
 * breadcrumb and a tab's menu cannot disagree about which chats are a tab's.
 */
import { chatsTree, type ChatRow, type ListedChat } from "./chatsTree";
import { homeOf, panesOf, shownIn, type AskedBy, type Tabs } from "./tabs";

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
 * whose home is that pane (`tabs.homeOf`), nested by who asked. A chat with a tab of its own
 * is not one of them, and neither is anything below it: those are that tab's.
 *
 * The one selector for "which chats does this tab hold": a tab's menu lists these rows, and
 * its count of working and finished tasks is a count over them.
 */
export function chatsOfTab(tabs: Tabs, id: number, chats: readonly ListedChat[]): ChatRow[] {
  return panesOf(tabs, id).flatMap(({ pane }) => chatsOfPane(tabs, id, pane, chats));
}

/** The chats of one pane of tab `id`, as {@link chatsOfTab} lists a tab's: the pane's own chat
 *  first, then the tasks at home in it. */
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
 * **The chats of tab `id` that are waiting for the person and are not on screen**, in the
 * order they wait: what the tab wears the hand for (V100-37).
 *
 * A chat is on screen only when its pane shows it AND its tab is in front: a task the person
 * left a tab on, and then went elsewhere, is not on screen when it asks. The session's own
 * chat counts when its pane shows another chat. While its pane shows it, the tab's own state
 * mark is that chat's, in front or behind, as it is for a session with no tasks.
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
    if (home === undefined || home.tab !== id) return false;
    const onPane = shown.get(home.pane) === session;
    return home.own === session ? !onPane : !(onPane && front);
  });
}
