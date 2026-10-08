import { createContext, memo, useContext, useLayoutEffect, useState, type ReactNode } from "react";
import { useSyncExternalStoreWithSelector } from "use-sync-external-store/with-selector";
import { useChatsHere, useChatsSelect } from "./chatState";
import { bucketsSaid, sameBuckets, type TaskBuckets } from "./taskBuckets";
import { atLimitOf, NONE_BELOW, sameBelow, taskCountOf, type TasksBelow } from "./taskCounts";

/*
 * **Each session's tasks, lent to whatever draws a session** (#1491, SC-3).
 *
 * The rows are `taskCounts.tasksBelowOf`'s, built once from the project's list of chats and its
 * finished rows. They are held here outside React, as the chats' states are (`chatState.ts`),
 * so a reader takes its own session's share and is drawn again only when that share changes:
 * a task that reports redraws its session's count and state, and no other row. A task that
 * only moves (a turn began, a turn ended) changes no list here at all; the count reads the
 * chats' states itself.
 */

type Listener = () => void;

/** Every session's tasks, and a way to hear when they change. */
type BelowStore = {
  get: () => ReadonlyMap<number, TasksBelow>;
  set: (next: ReadonlyMap<number, TasksBelow>) => void;
  subscribe: (listener: Listener) => () => void;
};

const NOTHING_BELOW: ReadonlyMap<number, TasksBelow> = new Map();

function store(): BelowStore {
  let held = NOTHING_BELOW;
  const listeners = new Set<Listener>();
  return {
    get: () => held,
    set: (next) => {
      if (next === held) return;
      held = next;
      for (const listener of [...listeners]) listener();
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

/** What is drawn outside a project's view, or in a test that lends nothing: no tasks. */
const NO_STORE = store();

const TasksBelowHere = createContext<BelowStore>(NO_STORE);

/**
 * Lends `below`, each session's tasks by its number (`tasksBelowOf`), to everything drawn
 * inside it. One of these per project view.
 */
export function TasksBelowLent({
  below,
  children,
}: {
  below: ReadonlyMap<number, TasksBelow>;
  children: ReactNode;
}) {
  const [held] = useState(store);
  // After the render that computed it, and before the screen is drawn: a reader whose share
  // changed is drawn again in the same frame.
  useLayoutEffect(() => held.set(below), [held, below]);
  return <TasksBelowHere.Provider value={held}>{children}</TasksBelowHere.Provider>;
}

/** Session `session`'s tasks: the same value until its own rows change. */
export function useTasksBelow(session: number | undefined): TasksBelow {
  const held = useContext(TasksBelowHere);
  return useSyncExternalStoreWithSelector(
    held.subscribe,
    held.get,
    held.get,
    (all) => (session === undefined ? NONE_BELOW : (all.get(session) ?? NONE_BELOW)),
    sameBelow,
  );
}

/**
 * **Session `session`'s count of its tasks** (`TaskBuckets`): how many are working, waiting,
 * done and failed, read from the rows the Chats list draws. The one count: a session's row says
 * it (`TaskCountShown`), `shownState` is handed its `working` as what the session waits on, and
 * a tab's chip and a folded row's summary read it here.
 *
 * Drawn again only when the count changes.
 */
export function useTaskCount(session: number | undefined): TaskBuckets {
  const below = useTasksBelow(session);
  return useChatsSelect(useChatsHere(), (states) => taskCountOf(states, below), sameBuckets);
}

/** What session `session`'s row says of its tasks (`bucketsSaid`), or nothing for a session with
 *  none. `6 of 6 tasks` at its limit. */
export function useTaskCountSaid(session: number | undefined): string | undefined {
  const below = useTasksBelow(session);
  return useChatsSelect(useChatsHere(), (states) =>
    bucketsSaid(taskCountOf(states, below), atLimitOf(states, below)),
  );
}

/**
 * **A session's count, on its row** (#1491): `2 working · 1 waiting · 3 done · 1 failed`.
 * Reads its own session's tasks and their states, so a task that changes state redraws this
 * and its own marks, and no row. Draws nothing for a session with no tasks.
 */
export const TaskCountShown = memo(function TaskCountShown({ session }: { session: number }) {
  const said = useTaskCountSaid(session);
  if (said === undefined) return null;
  return (
    <span className="task-count" data-testid={`task-count-${session}`}>
      {said}
    </span>
  );
});
