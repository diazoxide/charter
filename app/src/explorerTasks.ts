/**
 * **What the explorer says of tasks and helpers without listing them** (#1490, V100-4,
 * V100-14). The Chats list is the one tree of who asked whom; the explorer lists a workspace's
 * chats that have a tab of their own, and says the rest in counts:
 *
 * - a session's tasks are one line under its row, `5 tasks · 2 working · 3 done`, counted by
 *   the one count its row in the Chats list and its tab use (`taskCounts.ts`, `taskBuckets.ts`);
 * - a chat's harness helpers are a count on its row, `3 helpers · 1 working`, which unfolds.
 *
 * Nothing here is state and nothing here draws: which chats are rows, which are counted and
 * under whom, and what a count says. `ExplorerChats.tsx` draws it and `Explorer.tsx` places it.
 */
import { childrenOf, type ChatStates, type State } from "./chatState";
import type { ListedChat } from "./chatsTree";
import { shownState, type Shown, type ShownShape } from "./shownState";
import { bucketOfKind, bucketsSaid, type TaskBucket, type TaskBuckets } from "./taskBuckets";
import type { TasksBelow } from "./taskCounts";
import type { Token } from "./theme/theme";

/** Which of the project's chats are shown inside another chat's tab, and whose. */
export type Housed = {
  /** Each task that lives inside a tab, by its number: the chat whose tab that is. A chat
   *  that is not here has a row of its own in the explorer of the workspace it works in. */
  hostOf: ReadonlyMap<number, number>;
  /** The tasks living inside each chat's tab, at any depth below it, in the order listed. */
  tasksOf: ReadonlyMap<number, readonly ListedChat[]>;
};

/**
 * **Which chats live inside another chat's tab** (#1486), read off the project's list: a task
 * with no tab of its own whose asking chat is still open. Its host is the nearest chat above
 * it, by task links, that is not such a task itself: the session, or a task the person moved
 * to a tab of its own.
 *
 * Every other chat has a row: a session, a handoff, a shell, a task with a tab of its own, and
 * a task whose asking chat has closed, which nothing else in the explorer would say is running.
 */
export function housed(listed: readonly ListedChat[]): Housed {
  const by = new Map(listed.map((chat) => [chat.session, chat]));
  const inside = (chat: ListedChat) =>
    chat.mode === "task" &&
    !chat.tab &&
    chat.parent !== null &&
    chat.parent !== chat.session &&
    by.has(chat.parent);
  const hostOf = new Map<number, number>();
  const tasksOf = new Map<number, ListedChat[]>();
  for (const chat of listed) {
    if (!inside(chat)) continue;
    // Up the task links to the first chat with a row. A record that loops reaches none, and
    // every chat on the loop keeps a row of its own, so none is lost.
    const seen = new Set([chat.session]);
    let at = chat.parent === null ? undefined : by.get(chat.parent);
    while (at !== undefined && inside(at) && !seen.has(at.session)) {
      seen.add(at.session);
      at = at.parent === null ? undefined : by.get(at.parent);
    }
    if (at === undefined || inside(at)) continue;
    hostOf.set(chat.session, at.session);
    tasksOf.set(at.session, [...(tasksOf.get(at.session) ?? []), chat]);
  }
  return { hostOf, tasksOf };
}

/** `3 tasks`, `1 task`. */
export function tasksSaid(total: number): string {
  return `${total} ${total === 1 ? "task" : "tasks"}`;
}

/**
 * **A set of running tasks as the one count takes them** (`taskCounts.TasksBelow`): what the
 * line for the tasks working at a place counts. A session's own line is handed the session's
 * rows as its row in the Chats list has them (`tasksBelowOf`); this is for a set that is no
 * one session's. Each task's asker is the chat that asked for it, so a task waiting on tasks
 * of its own that are in the set reads as working, as on its row.
 */
export function belowOf(tasks: readonly ListedChat[]): TasksBelow {
  return {
    open: tasks.map((task) => ({
      session: task.session,
      asker: task.parent ?? task.session,
      shell: task.shell,
      direct: false,
      report: task.report,
      outcome: task.outcome,
      asking: task.asking,
      harness: task.harness,
    })),
    finished: [],
    limit: null,
  };
}

/** One part of what a line says after how many: a bucket's count, with the mark its state
 *  has on a row. */
export type CountPart = { bucket: TaskBucket; said: string; shape: ShownShape; token: Token };

/** The mark of each bucket: the shape and the colour its state has on a row. */
const BUCKET_MARKS: Readonly<Record<TaskBucket, Pick<CountPart, "shape" | "token">>> = {
  working: { shape: "ring", token: "state.running" },
  waiting: { shape: "pause", token: "text.muted" },
  done: { shape: "tick", token: "text.muted" },
  failed: { shape: "cross", token: "state.failed" },
};

/**
 * **What a line says of a count, part by part**: the words are `bucketsSaid`'s, the one
 * sentence a session's row and its tab say too, split where it joins them so each part can be
 * drawn with its state's mark (a word and a shape, never a colour alone).
 */
export function countParts(count: TaskBuckets): CountPart[] {
  const said = bucketsSaid(count);
  if (said === undefined) return [];
  return said.split(" · ").flatMap((part) => {
    const bucket = (["working", "waiting", "done", "failed"] as const).find((one) =>
      part.endsWith(` ${one}`),
    );
    return bucket === undefined ? [] : [{ bucket, said: part, ...BUCKET_MARKS[bucket] }];
  });
}

/** The task among `tasks` that has needed the person longest, or none. */
export function firstNeeding(
  needsYou: readonly number[],
  tasks: readonly ListedChat[],
): number | undefined {
  if (needsYou.length === 0 || tasks.length === 0) return undefined;
  return needsYou.find((session) => tasks.some((task) => task.session === session));
}

/** `3 helpers`, `1 helper`. */
export function helpersSaid(count: number): string {
  return `${count} ${count === 1 ? "helper" : "helpers"}`;
}

/** How a chat's helpers stand: how many it has had, and how many of them are working and
 *  have failed now. */
export type HelpersCount = { total: number; working: number; failed: number };

/**
 * **A chat's helpers, counted by how they stand.** The core keeps a helper that has ended, so
 * the total only grows over a conversation: said alone, `40 helpers` would read as forty at
 * work. Each is put in the bucket a task in its state is in (`bucketOfKind`), and the working
 * and the failed are said.
 */
export function helpersCountOf(states: ChatStates, session: number): HelpersCount {
  const count = { total: 0, working: 0, failed: 0 };
  for (const child of childrenOf(states, session)) {
    count.total += 1;
    const bucket = bucketOfKind(helperShown(child.state).kind);
    if (bucket === "working") count.working += 1;
    else if (bucket === "failed") count.failed += 1;
  }
  return count;
}

/** Whether two counts of helpers say the same. */
export function sameHelpersCount(one: HelpersCount, other: HelpersCount): boolean {
  return one.total === other.total && one.working === other.working && one.failed === other.failed;
}

/** What is said after how many helpers: `2 working`, `1 failed`, each only when there are
 *  any. Nothing when every one has ended well. */
export function helpersStand(count: HelpersCount): string[] {
  return [
    count.working > 0 ? `${count.working} working` : undefined,
    count.failed > 0 ? `${count.failed} failed` : undefined,
  ].filter((part) => part !== undefined);
}

/** `40 helpers · 2 working · 1 failed`: what a row with no room to unfold them says. */
export function helpersCountSaid(count: HelpersCount): string {
  return [helpersSaid(count.total), ...helpersStand(count)].join(" · ");
}

/** A chat's helpers as the tree needs them: that it has some, and which while unfolded. */
export type HelpersOf = {
  session: number;
  /** The harness's ids for them, oldest first, while the count is unfolded; nothing while it
   *  is folded, when only the count's own line reads how many. */
  agents: readonly string[] | null;
};

/**
 * **The helpers the tree draws rows for**: each of `sessions` that has any, with their ids
 * where its count is unfolded. A folded chat's helpers coming and going changes nothing here
 * until the first comes or the last goes, so the tree is not drawn again for them.
 */
export function helpersOf(
  states: ChatStates,
  sessions: readonly number[],
  unfolded: ReadonlySet<number>,
): HelpersOf[] {
  return sessions.flatMap((session) => {
    const children = childrenOf(states, session);
    if (children.length === 0) return [];
    return [
      { session, agents: unfolded.has(session) ? children.map((child) => child.agent) : null },
    ];
  });
}

/** Whether two answers of {@link helpersOf} say the same. */
export function sameHelpers(one: readonly HelpersOf[], other: readonly HelpersOf[]): boolean {
  return (
    one === other ||
    (one.length === other.length &&
      one.every((mine, at) => {
        const theirs = other[at];
        if (mine.session !== theirs.session) return false;
        if (mine.agents === null || theirs.agents === null) return mine.agents === theirs.agents;
        return (
          mine.agents.length === theirs.agents.length &&
          mine.agents.every((agent, i) => agent === theirs.agents?.[i])
        );
      }))
  );
}

/** The fewest characters of a helper's id its row shows. */
const SHORT_ID = 8;

/**
 * **Each helper's short id**, by its whole one: the first eight characters, which is what a
 * person can read of a hash, and more of them only where two of the same chat's helpers would
 * otherwise read the same. An id that fits, or nearly, is shown whole. The whole id is the row's tooltip.
 */
export function shortIds(agents: readonly string[]): ReadonlyMap<string, string> {
  const longest = Math.max(0, ...agents.map((agent) => agent.length));
  let shown = SHORT_ID;
  // Cut only where it saves something: an id a character or two over is shown whole.
  const cut = (agent: string) => (agent.length > shown + 2 ? `${agent.slice(0, shown)}…` : agent);
  while (shown < longest && new Set(agents.map(cut)).size < new Set(agents).size) shown += 4;
  return new Map(agents.map((agent) => [agent, cut(agent)]));
}

/** The states a harness reports of a helper that the board has a word for. */
const HELPER_STATES: readonly string[] = ["running", "waiting", "done", "failed"];

/**
 * **A helper's state, as its harness reports it, in the words every row uses**: working, done,
 * failed, idle. A word this window does not know is said as it came, with the mark of what is
 * not known: nothing is guessed. A helper is never said to need you: its asks are its chat's.
 */
export function helperShown(state: string): Shown {
  const known = HELPER_STATES.includes(state)
    ? shownState({ board: state as State, needsYou: false, task: null, harness: null })
    : undefined;
  return (
    known ?? {
      kind: "unheard",
      word: state === "" || state === "unknown" ? "running (no detail)" : state,
      shape: "dots",
      token: "text.muted",
    }
  );
}
