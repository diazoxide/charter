/**
 * **What a task asks of the person, as its session's tab says it** (#1508, V100-56, V100-57).
 *
 * - **Whose it is, by its whole path**: `“deep” (a task of “steward 4” › “talk”)`. The path's
 *   shape is read from the core's list of who asked whom (`ListedChat.parent`, which the app
 *   recorded when it started each task), never from anything a chat says about itself. The
 *   names on it are chosen by chats, so each is quoted ({@link named}): a name cannot draw a
 *   step of the path, a quote or a `›` of purlis's.
 * - **One question for the tasks that hit the same block**: several tasks below one session
 *   blocked on the same host, or on writing the same folder, are grouped
 *   ({@link taskBlockGroups}) and asked about in one Notice, whose one answer applies to each
 *   task it lists and to no other chat. The session's own block is never in a group: a
 *   permission given to a session does not reach its tasks.
 */
import type { ChatBlocked, GrantLevel } from "./bindings";
import type { ListedChat } from "./chatsTree";
import type { Blocks } from "./sandboxBlocks";

/**
 * **A chat's name inside a path**, quoted: `“talk”`. The marks the path is drawn with are
 * purlis's alone, so a name's own `“`, `”` and `›` are shown as `'`, `'` and `>`.
 */
export function named(name: string): string {
  return `“${name.replace(/[“”]/g, "'").replace(/›/g, ">")}”`;
}

/**
 * **Whose a Notice of `chat` is, said on its session's tab**: the session's own name for the
 * session, and for a task its name with the path above it, each name quoted: `“talk” (a task
 * of “steward 4”)`, `“deep” (a task of “steward 4” › “talk”)`. Where the path does not reach
 * `own` in the core's list (a link it does not hold), only the session is named, never a guess.
 */
export function whoseOf(chat: ListedChat, own: ListedChat, chats: readonly ListedChat[]): string {
  if (chat.session === own.session) return own.name;
  const byNumber = new Map(chats.map((one) => [one.session, one]));
  const above: string[] = [];
  const seen = new Set<number>([chat.session]);
  let at: ListedChat | undefined = chat;
  while (at !== undefined) {
    const parent: number | null = at.mode === "task" ? at.parent : null;
    if (parent === null || seen.has(parent)) break;
    seen.add(parent);
    if (parent === own.session)
      return `${named(chat.name)} (a task of ${[own.name, ...above].map(named).join(" › ")})`;
    at = byNumber.get(parent);
    if (at !== undefined) above.unshift(at.name);
  }
  return `${named(chat.name)} (a task of ${named(own.name)})`;
}

/** One task in a question its session's tab asks for several. */
export type Member = { session: number; whose: string; block: ChatBlocked };

/** **Several tasks of one session blocked on the same thing**, asked about in one Notice. */
export type TaskBlockGroup = {
  /** What it is known by: the offer, the operation, the kind and the target. */
  key: string;
  /** The session whose tasks these are. */
  session: number;
  offer: "host" | "write";
  /** The host, or the folder, shown whole, as a grant matches it ({@link matched}): what an
   *  Allow grants. */
  target: string;
  /** What was blocked, in purlis's words. */
  said: string;
  /** The levels every task's block may be allowed at (#1343). */
  levels: GrantLevel[];
  /** The tasks, in the order the tab lists them. Two or more. */
  members: Member[];
};

/**
 * **What a block names, as a grant matches it**, the core's rule (`taskblocks::normalised`): a
 * host lowered, without a trailing dot and without the default port (`:443`, `:80`); a folder
 * as the core offered it.
 */
export function matched(offer: "host" | "write", target: string): string {
  if (offer === "write") return target;
  const host = target
    .trim()
    .toLowerCase()
    .replace(/:(443|80)$/, "");
  return host.endsWith(".") ? host.slice(0, -1) : host;
}

/** The question a block is asked under, or none for one that is answered on its own. */
function keyOf(block: ChatBlocked): string | undefined {
  if (block.ours || block.target === null) return undefined;
  if (block.offer !== "host" && block.offer !== "write") return undefined;
  const target = matched(block.offer, block.target);
  return `${block.offer}\u0000${block.operation}\u0000${block.kind}\u0000${target}`;
}

/**
 * **The questions the tab of `session` asks for several of its tasks at once**: each block one
 * of `tasks` holds that offers an Allow and names its host or folder, grouped by what it
 * names. A group of one is not a group: that task's own Notice asks it.
 */
export function taskBlockGroups(
  blocks: Blocks,
  session: number,
  tasks: readonly { session: number; whose: string }[],
): TaskBlockGroup[] {
  const groups = new Map<string, TaskBlockGroup>();
  for (const task of tasks) {
    if (task.session === session) continue;
    for (const block of blocks[task.session] ?? []) {
      const key = keyOf(block);
      if (key === undefined || block.target === null) continue;
      const group = groups.get(key) ?? {
        key,
        session,
        offer: block.offer as "host" | "write",
        target: matched(block.offer as "host" | "write", block.target),
        said: block.said,
        levels: [...block.levels],
        members: [],
      };
      group.levels = group.levels.filter((level) => block.levels.includes(level));
      group.members.push({ session: task.session, whose: task.whose, block });
      groups.set(key, group);
    }
  }
  return [...groups.values()].filter((group) => group.members.length > 1);
}

/** `blocks` without the ones `groups` ask about: each is asked once, in its group. */
export function withoutGrouped(blocks: Blocks, groups: readonly TaskBlockGroup[]): Blocks {
  if (groups.length === 0) return blocks;
  const asked = new Set(groups.flatMap((group) => group.members.map((one) => one.block)));
  const left: Record<number, readonly ChatBlocked[]> = {};
  for (const [session, held] of Object.entries(blocks)) {
    const kept = held.filter((block) => !asked.has(block));
    if (kept.length > 0) left[Number(session)] = kept;
  }
  return left;
}

/** `talk, sweep and probe`. */
export function listed(names: readonly string[]): string {
  if (names.length < 2) return names.join("");
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}
