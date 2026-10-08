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
  const parents = new Map(chats.map((chat) => [chat.session, chat.parent]));
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
  const askedBy = askedByOf(chats);
  const orphaned = new Map(chatsTree(chats).map((row) => [row.session, row.orphaned]));
  return panesOf(tabs, id).flatMap(({ pane }) => {
    const here = chats.filter((chat) => {
      const home = homeOf(tabs, chat.session, askedBy);
      return home !== undefined && home.tab === id && home.pane === pane;
    });
    // Nested among themselves: the pane's own chat is the only one whose asker is not here.
    return chatsTree(here).map((row) => ({
      ...row,
      orphaned: orphaned.get(row.session) ?? false,
    }));
  });
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
      at = at.parent === null ? undefined : byNumber.get(at.parent);
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
 * **The chats of tab `id` that need you and are not on screen in it**, longest waiting first:
 * what the tab wears the needs-you mark for. The session's own chat is not one of them: the
 * tab's own state mark is that chat's.
 */
export function hiddenNeeding(
  tabs: Tabs,
  id: number,
  askedBy: AskedBy,
  /** The chats asking for you, oldest first. */
  needsYou: readonly number[],
): number[] {
  if (needsYou.length === 0) return [];
  const shown = new Map(shownIn(tabs, id).map((one) => [one.pane, one.session]));
  return needsYou.filter((session) => {
    const home = homeOf(tabs, session, askedBy);
    return (
      home !== undefined &&
      home.tab === id &&
      home.own !== session &&
      shown.get(home.pane) !== session
    );
  });
}
