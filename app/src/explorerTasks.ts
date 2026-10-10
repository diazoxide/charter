/**
 * **What a chat's row says of its harness's helpers** (#1490, V100-14): a count,
 * `3 helpers · 1 working`, drawn by `ExplorerChats.tsx`'s `HelpersSaid` on the chat's row in
 * the Chats list. Nothing here is state and nothing here draws.
 *
 * The explorer listed chats, their tasks and their helpers as rows of its own until #1673; what
 * served only those rows went with them (#1686).
 */
import { childrenOf, type ChatStates, type State } from "./chatState";
import { shownState, type Shown } from "./shownState";
import { taskBucketOf } from "./taskBuckets";

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
 * work. Each is put in the bucket a task in its state is in (`taskBucketOf`), and the working
 * and the failed are said.
 */
export function helpersCountOf(states: ChatStates, session: number): HelpersCount {
  const count = { total: 0, working: 0, failed: 0 };
  for (const child of childrenOf(states, session)) {
    count.total += 1;
    const bucket = taskBucketOf(helperShown(child.state).kind);
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
function helpersStand(count: HelpersCount): string[] {
  return [
    count.working > 0 ? `${count.working} working` : undefined,
    count.failed > 0 ? `${count.failed} failed` : undefined,
  ].filter((part) => part !== undefined);
}

/** `40 helpers · 2 working · 1 failed`: what a row with no room to unfold them says. */
export function helpersCountSaid(count: HelpersCount): string {
  return [helpersSaid(count.total), ...helpersStand(count)].join(" · ");
}

/** The states a harness reports of a helper that the board has a word for. */
const HELPER_STATES: readonly string[] = ["running", "waiting", "done", "failed"];

/**
 * **A helper's state, as its harness reports it, in the words every row uses**: working, done,
 * failed, idle. A word this window does not know is said as it came, with the mark of what is
 * not known: nothing is guessed. A helper is never said to need you: its asks are its chat's.
 */
function helperShown(state: string): Shown {
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
