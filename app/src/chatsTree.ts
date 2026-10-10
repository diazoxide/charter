/**
 * **The project's running chats as one tree** (#1447): which chat asked which for a task,
 * whatever workspace each works in.
 *
 * **Only a task is nested** (#1492, V100-69). A handoff moved the work: its chat is a session
 * of its own, with its own tab, and stands at the top of the tree. It is under no chat, so it
 * is in no chat's counts, summary or needs-you mark, and nothing that ends a chat "and
 * everything below it" ends it. Where it came from is said on its own row, and where the work
 * went on the row of the chat it came from ({@link handedOff}).
 *
 * The lineage is the core's: a chat another chat started carries that chat's number, the mode
 * it was started in and whether it has a tab (`OpenChat.from`). This file only shapes it, so
 * the Chats section and the explorer draw the same parent for the same chat.
 */
import type { Placed } from "./tabs";
import type { AtLimit, OpenChat } from "./bindings";
import { isShell } from "./chatState";
import { rowFactsOf, type RowFacts } from "./shownState";

/** What a task the person asked for themselves says on its row and in its breadcrumb (#1492,
 *  V100-70). */
export const ASKED_BY_YOU = "asked by you";

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
   *  it, in a folder of its own, and only the person merges it (#1511). */
  branch: string | null;
  /** The most tasks it may have running at once, by the limits in force for it, where the
   *  core said: only for a chat that has a task open (#1491, V100-26). */
  tasksLimit?: number | null;
  /** How many tasks it has running against that limit, as the core counts them for a
   *  dispatch from it (#1491). */
  tasksRunning?: number | null;
  /** Where its last dispatch was refused for a limit a slot frees, and none has freed since
   *  (#1498, V100-54): the number that binds, and the core's sentence saying which limit and
   *  where it is changed. Its row says `at its task limit` while it is set. */
  atLimit?: AtLimit | null;
  /** How many of its dispatches wait until this machine has memory to spare (#1617), where
   *  any does: its row says so, as the Dispatches tab's *Not started* list names them. */
  waitingOnMemory?: number | null;
  /** A task the person asked for themselves, from its session's tab (#1492, V100-70): its row
   *  and its breadcrumb say `asked by you`. */
  byYou?: boolean;
  /** Whether its harness is one purlis types a line of its own into (`typedInto`). Not said
   *  for a chat nothing was read of: only a plain `false` takes an offer away. */
  typed?: boolean;
  /** What it runs on: its profile and the harness kind the profile declares, `work (claude)`,
   *  or the harness alone where it has no profile (#1673). A profile is what the person picked
   *  and what a relaunch looks up again; the kind is what the project calls the harness. The
   *  explorer's row said it until the explorer stopped listing chats; the row's hover does. */
  runsOn?: string | null;
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
 * Whether purlis types a line of its own into harness `harness`, by the core's name for it
 * (`purlis_core::dispatched::told_by_a_line`): Claude Code and Codex. On any other, and in a
 * chat with none, it types nothing, so a task there cannot be given a turn to report (V100-71).
 * The half of that rule that never changes while a chat runs; the rest (a prompt, the person's
 * keys, a harness not yet heard from) is the core's to say when it is asked.
 */
export function typedInto(harness: string | null): boolean {
  return harness === "claude" || harness === "codex";
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
  /**
   * Where it is, for a task listed among the chats of its session's tab though it has a pane
   * of its own (#1489, `tabs.Placed`): in a tab of its own, or beside its session. Only a
   * tab's own list says it (`tabChats.chatsOfTab`); the Chats list's rows never do.
   */
  placed?: Placed;
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
    tasksLimit: chat.tasks_limit ?? null,
    tasksRunning: chat.tasks_running ?? null,
    atLimit: chat.at_limit ?? null,
    waitingOnMemory: chat.waiting_on_memory ?? null,
    byYou: chat.from?.task === true && chat.from.by_person === true,
    typed: typedInto(chat.harness),
    runsOn: chat.profile
      ? chat.harness
        ? `${chat.profile} (${chat.harness})`
        : chat.profile
      : chat.harness,
    ...rowFactsOf(chat, nameOf),
  };
}

/** The chats each chat started, as a task or by a handoff, by its number, in the order they
 *  are listed. */
export function startedBy(chats: readonly ListedChat[]): ReadonlyMap<number, ListedChat[]> {
  const open = new Set(chats.map((chat) => chat.session));
  const by = new Map<number, ListedChat[]>();
  for (const chat of chats) {
    if (chat.parent === null || !open.has(chat.parent) || chat.parent === chat.session) continue;
    by.set(chat.parent, [...(by.get(chat.parent) ?? []), chat]);
  }
  return by;
}

/** Whether `chat` is nested under an open chat: a task whose asking chat is listed. */
function nested(chat: ListedChat, open: ReadonlySet<number>): boolean {
  return (
    chat.mode === "task" &&
    chat.parent !== null &&
    chat.parent !== chat.session &&
    open.has(chat.parent)
  );
}

/**
 * **The tasks each chat asked for**, by its number, in the order they are listed: the rows
 * nested under its row, and what "everything below it" means. A handoff is not one (#1492).
 */
export function tasksOf(chats: readonly ListedChat[]): ReadonlyMap<number, ListedChat[]> {
  const open = new Set(chats.map((chat) => chat.session));
  const by = new Map<number, ListedChat[]>();
  for (const chat of chats) {
    if (chat.parent === null || !nested(chat, open)) continue;
    by.set(chat.parent, [...(by.get(chat.parent) ?? []), chat]);
  }
  return by;
}

/**
 * **Where each chat's work went by a handoff** (#1492, V100-69), by the number of the chat it
 * came from: the chats it handed off to that are still open, the newest first. What that
 * chat's row says, `handed off to <chat>`, and where a press on it goes.
 */
export function handedOff(chats: readonly ListedChat[]): ReadonlyMap<number, ListedChat[]> {
  const to = new Map<number, ListedChat[]>();
  for (const chat of chats) {
    if (chat.mode !== "handoff" || chat.parent === null || chat.parent === chat.session) continue;
    to.set(chat.parent, [...(to.get(chat.parent) ?? []), chat]);
  }
  // By number, which is the order they were started in: the last one started is first.
  for (const went of to.values()) went.sort((a, b) => b.session - a.session);
  return to;
}

/** `handed off to drop commons`, and `and 2 more` where the work went to several chats. */
export function handedOffSaid(newest: string, more: number): string {
  return more === 0 ? `handed off to ${newest}` : `handed off to ${newest} and ${more} more`;
}

/**
 * The rows of the tree, top to bottom: each chat, then the tasks it asked for, nested under it.
 *
 * **A chat whose parent has closed stands at the top**, marked `orphaned`: it is still running,
 * and the tree must not lose it with the row it hung from. Every chat is drawn exactly once,
 * whatever the lineage says: a chain that loops back on itself is cut at the first chat seen.
 *
 * **A handoff stands at the top whatever became of the chat it came from** (#1492), and is
 * `orphaned` only once that chat has closed.
 */
export function chatsTree(chats: readonly ListedChat[]): ChatRow[] {
  const children = tasksOf(chats);
  const open = new Set(chats.map((chat) => chat.session));
  const rows: ChatRow[] = [];
  const seen = new Set<number>();
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
        // Only where its parent is not open (#1492, #1513): a handoff whose chat is open stands
        // at the top unmarked, and a loop in the lineage marks nothing.
        orphaned: orphans && chat.parent !== null && !open.has(chat.parent),
      });
      walk(level + 1, children.get(chat.session) ?? [], false);
    });
  };
  walk(
    1,
    chats.filter((chat) => !nested(chat, open)),
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
