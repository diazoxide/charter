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
  /** Whether it has a tab. A task chat has none until its row is clicked. */
  tab: boolean;
};

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
    tab,
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
        orphaned: orphans && chat.parent !== null,
      });
      walk(level + 1, children.get(chat.session) ?? [], false);
    });
  };
  const open = new Set(chats.map((chat) => chat.session));
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
