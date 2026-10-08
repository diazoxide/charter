/**
 * **What the explorer says of tasks and helpers without listing them** (#1490, V100-4,
 * V100-14). The Chats list is the one tree of who asked whom; the explorer lists a workspace's
 * chats that have a tab of their own, and says the rest in counts:
 *
 * - a session's tasks are one line under its row, `5 tasks 2 working 3 done`;
 * - a chat's harness helpers are a count on its row, `3 helpers`, which unfolds.
 *
 * Nothing here is state and nothing here draws: which chats are rows, which are counted and
 * under whom, and what a count says. `ExplorerChats.tsx` draws it and `Explorer.tsx` places it.
 */
import type { FinishedTask } from "./bindings";
import { childrenOf, markOf, type ChatStates, type State } from "./chatState";
import type { ListedChat } from "./chatsTree";
import { shownOf } from "./finished";
import { shownState, type Shown, type ShownKind } from "./shownState";

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

/**
 * The finished tasks counted on a chat's line: its own, and those of each task living inside
 * its tab. They are entries under those chats in the Chats list until they are cleared.
 */
export function endedUnder(
  finished: ReadonlyMap<number, readonly FinishedTask[]>,
  host: number,
  tasks: readonly ListedChat[],
): FinishedTask[] {
  return [host, ...tasks.map((task) => task.session)].flatMap(
    (session) => finished.get(session) ?? [],
  );
}

/** How many tasks there are, and how many of them are in each state the line says. */
export type TaskCounts = {
  total: number;
  working: number;
  needsYou: number;
  done: number;
  failed: number;
};

/** The states the line counts, each by the kinds of shown state it holds. A task that ended
 *  without a report did not do the work, and is counted with the failed. */
const COUNTED: Readonly<Partial<Record<ShownKind, keyof Omit<TaskCounts, "total">>>> = {
  working: "working",
  "needs-you": "needsYou",
  done: "done",
  failed: "failed",
  unreported: "failed",
};

/**
 * **How many of these tasks are in each state**, by the one function every row's state comes
 * from (`shownState`): the running ones from what the board says of them now, the finished
 * ones from how they ended. A task in a state the line does not name (idle, asking its asker,
 * cancelled) is in the total only.
 *
 * Written here because no count over a session's tasks exists on this branch; a later one in
 * a module of its own replaces it.
 */
export function countTasks(
  states: ChatStates,
  open: readonly ListedChat[],
  ended: readonly FinishedTask[] = [],
): TaskCounts {
  const counts: TaskCounts = {
    total: open.length + ended.length,
    working: 0,
    needsYou: 0,
    done: 0,
    failed: 0,
  };
  const count = (shown: Shown | undefined) => {
    const as = shown === undefined ? undefined : COUNTED[shown.kind];
    if (as !== undefined) counts[as] += 1;
  };
  for (const task of open)
    count(
      shownState({
        board: markOf(states, task.session, task.shell),
        needsYou: states.needsYou.includes(task.session),
        task:
          task.report === null
            ? null
            : { report: task.report, outcome: task.outcome, asking: task.asking },
        harness: task.harness,
      }),
    );
  for (const task of ended) count(shownOf(task));
  return counts;
}

/** Whether two counts say the same. */
export function sameCounts(one: TaskCounts, other: TaskCounts): boolean {
  return (
    one.total === other.total &&
    one.working === other.working &&
    one.needsYou === other.needsYou &&
    one.done === other.done &&
    one.failed === other.failed
  );
}

/** `3 tasks`, `1 task`. */
export function tasksSaid(total: number): string {
  return `${total} ${total === 1 ? "task" : "tasks"}`;
}

/** What the line says after how many there are, in the state vocabulary: each state that has
 *  any, working first. A state with none is not said. */
export function countsSaid(counts: TaskCounts): { state: ShownKind; said: string }[] {
  const said: { state: ShownKind; said: string }[] = [];
  if (counts.working > 0) said.push({ state: "working", said: `${counts.working} working` });
  if (counts.needsYou > 0)
    said.push({
      state: "needs-you",
      said: `${counts.needsYou} ${counts.needsYou === 1 ? "needs" : "need"} you`,
    });
  if (counts.done > 0) said.push({ state: "done", said: `${counts.done} done` });
  if (counts.failed > 0) said.push({ state: "failed", said: `${counts.failed} failed` });
  return said;
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
