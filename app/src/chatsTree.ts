/**
 * **The project's running chats as one tree** (#1447): which chat started which, whatever
 * workspace each works in.
 *
 * The lineage is the core's: a chat another chat started carries that chat's number, the mode
 * it was started in and whether it has a tab (`OpenChat.from`). This file only shapes it, so
 * the Chats section and the explorer draw the same parent for the same chat.
 */
import type { OpenChat } from "./bindings";
import { isShell } from "./chatState";
import { rowFactsOf, type RowFacts } from "./shownState";

/** One running chat, as the Chats section lists it. */
export type ListedChat = {
  session: number;
  /** As its tab names it, or would. */
  name: string;
  persona: string | null;
  /** Where it works: a workspace's name, or the root's own word. */
  workspace: string;
  /** A shell tab, which draws no state until something reports one. */
  shell: boolean;
  /** The chat that started it, by number, where one did. */
  parent: number | null;
  /** How another chat started it: the work moved, or a report is owed. None for a chat a
   *  person opened. */
  mode: "handoff" | "task" | null;
  /** That chat's name as the person saw it then: what is said once it has closed. */
  from: string | null;
  /** That chat is not open because a launch could not start it, and it waits to: it has not
   *  closed (#1513). */
  askerWaiting?: boolean;
  /** Whether it has a tab. A task has none until its row is clicked. */
  tab: boolean;
  /** The branch of its own a task works on, where its dispatch gave it one (#1453): purlis cut
   *  it, in a folder of its own, and nothing merges it. */
  branch: string | null;
  // And what its state is derived from beside the board's word (`RowFacts`): its record as a
  // task and its harness's name.
} & RowFacts;

/**
 * **The branch of its own a task chat works on**, read from where it stands: a folder purlis
 * cut for it under its workspace's `.worktrees/<repo>/`, whose name is the branch's. Only a
 * task: a chat the person started in a branch's folder is not a dispatch's, and the explorer
 * already draws it under that branch.
 */
export function ownBranch(chat: OpenChat): string | null {
  if (!chat.from?.task || chat.cwd === null) return null;
  const at = /\/workspaces\/[^/]+\/\.worktrees\/[^/]+\/([^/]+)(?:\/|$)/.exec(chat.cwd);
  return at?.[1] ?? null;
}

/**
 * What a row at the top says of the chat it came from, which is not open: a task was asked by
 * it (V100-64, #1513), and the work a handoff moved came from it. "(closed)" only where it has
 * closed: one a launch could not start waits to, and is "(not open)".
 */
export function cameFromSaid(from: string, task: boolean, waiting = false): string {
  if (!task) return `from ${from}`;
  return waiting ? `asked by ${from} (not open)` : `asked by ${from} (closed)`;
}

/** A listed chat at its place in the tree. */
export type ChatRow = ListedChat & {
  /** 1 at the top. */
  level: number;
  posinset: number;
  setsize: number;
  /** Its parent has closed, so it stands at the top and says where it came from. */
  orphaned: boolean;
};

/** One of the core's open chats as the section lists it. `tab` is the window's own answer. */
export function listedChat(
  chat: OpenChat,
  workspace: string,
  name: string,
  tab: boolean,
  /** What a chat is called now, by its number (`taskFactsOf`). */
  nameOf?: (session: number) => string | undefined,
): ListedChat {
  return {
    session: chat.session,
    name,
    persona: chat.persona,
    workspace,
    shell: isShell(chat),
    parent: chat.from?.chat ?? null,
    mode: chat.from ? (chat.from.task ? "task" : "handoff") : null,
    from: chat.from?.name ?? null,
    askerWaiting: chat.from?.asker_waiting === true,
    tab,
    branch: ownBranch(chat),
    ...rowFactsOf(chat, nameOf),
  };
}

/** The chats each chat started, by its number, in the order they are listed. */
export function startedBy(chats: readonly ListedChat[]): ReadonlyMap<number, ListedChat[]> {
  const open = new Set(chats.map((chat) => chat.session));
  const by = new Map<number, ListedChat[]>();
  for (const chat of chats) {
    if (chat.parent === null || !open.has(chat.parent) || chat.parent === chat.session) continue;
    by.set(chat.parent, [...(by.get(chat.parent) ?? []), chat]);
  }
  return by;
}

/**
 * The chats each chat started that work in **another workspace** than it does, by its number:
 * what the explorer draws under the asking chat's row with the workspace named, since nothing
 * else in that workspace's explorer would say where they went.
 */
export function startedElsewhere(chats: readonly ListedChat[]): ReadonlyMap<number, ListedChat[]> {
  const at = new Map(chats.map((chat) => [chat.session, chat.workspace]));
  return new Map(
    [...startedBy(chats)]
      .map(([asker, started]) => {
        const away = started.filter((chat) => chat.workspace !== at.get(asker));
        return [asker, away] as const;
      })
      .filter(([, away]) => away.length > 0),
  );
}

/**
 * The rows of the tree, top to bottom: each chat, then the chats it started, nested under it.
 *
 * **A chat whose parent has closed stands at the top**, marked `orphaned`: it is still running,
 * and the tree must not lose it with the row it hung from. Every chat is drawn exactly once,
 * whatever the lineage says: a chain that loops back on itself is cut at the first chat seen.
 */
export function chatsTree(chats: readonly ListedChat[]): ChatRow[] {
  const children = startedBy(chats);
  const rows: ChatRow[] = [];
  const seen = new Set<number>();
  const open = new Set(chats.map((chat) => chat.session));
  const walk = (level: number, among: readonly ListedChat[], orphans: boolean) => {
    const fresh = among.filter((chat) => !seen.has(chat.session));
    fresh.forEach((chat, at) => {
      if (seen.has(chat.session)) return;
      seen.add(chat.session);
      rows.push({
        ...chat,
        level,
        posinset: at + 1,
        setsize: fresh.length,
        // Only where its parent is not open: a loop in the lineage stands at the top too.
        orphaned: orphans && chat.parent !== null && !open.has(chat.parent),
      });
      walk(level + 1, children.get(chat.session) ?? [], false);
    });
  };
  walk(
    1,
    chats.filter(
      (chat) => chat.parent === null || !open.has(chat.parent) || chat.parent === chat.session,
    ),
    true,
  );
  // Whatever a loop in the lineage left out, at the top, so it is still listed.
  walk(
    1,
    chats.filter((chat) => !seen.has(chat.session)),
    true,
  );
  return rows;
}

/** The chats that have a row nested under theirs: the rows that fold. */
export function parentsIn(rows: readonly ChatRow[]): ReadonlySet<number> {
  const parents = new Set<number>();
  rows.forEach((row, at) => {
    if ((rows[at + 1]?.level ?? 0) > row.level) parents.add(row.session);
  });
  return parents;
}

/** The rows drawn when the chats in `folded` are folded: every row but the ones nested, at any
 *  depth, under a folded one. */
export function unfolded(rows: readonly ChatRow[], folded: ReadonlySet<number>): ChatRow[] {
  const drawn: ChatRow[] = [];
  /** The level of the folded row being skipped under, while one is. */
  let under: number | undefined;
  for (const row of rows) {
    if (under !== undefined && row.level > under) continue;
    under = folded.has(row.session) ? row.level : undefined;
    drawn.push(row);
  }
  return drawn;
}

/**
 * **Where each row's needs-you mark leads** (#1448), by the row's chat: the chat itself while
 * it needs you, and otherwise the chat below it, at any depth, that has needed you longest.
 * A row that is in neither case is not here, and draws no mark.
 *
 * **Read off every row, drawn or not**, so a folded row answers for what it hides: a chat two
 * levels down that needs you marks every row above it, and a fold cannot hide it.
 */
export function needing(
  rows: readonly ChatRow[],
  /** The chats asking for you, oldest first. */
  needsYou: readonly number[],
): ReadonlyMap<number, number> {
  const leads = new Map<number, number>();
  if (needsYou.length === 0) return leads;
  const waited = new Map(needsYou.map((session, at) => [session, at]));
  rows.forEach((row, at) => {
    if (waited.has(row.session)) {
      leads.set(row.session, row.session);
      return;
    }
    let longest: { session: number; turn: number } | undefined;
    for (let next = at + 1; next < rows.length && rows[next].level > row.level; next += 1) {
      const turn = waited.get(rows[next].session);
      if (turn !== undefined && (longest === undefined || turn < longest.turn))
        longest = { session: rows[next].session, turn };
    }
    if (longest !== undefined) leads.set(row.session, longest.session);
  });
  return leads;
}

/** The running chats nested under `session`, at any depth, top to bottom. */
export function below(chats: readonly ListedChat[], session: number): number[] {
  const rows = chatsTree(chats);
  const at = rows.findIndex((row) => row.session === session);
  if (at < 0) return [];
  const under: number[] = [];
  for (let next = at + 1; next < rows.length && rows[next].level > rows[at].level; next += 1)
    under.push(rows[next].session);
  return under;
}

/** How many running chats are nested under `session`, at any depth: what stopping it and
 *  everything below it would end beside the chat itself. */
export function belowCount(chats: readonly ListedChat[], session: number): number {
  return below(chats, session).length;
}
