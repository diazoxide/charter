import { memo } from "react";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { Hand, ListTree } from "lucide-react";
import type { FinishedTask } from "./bindings";
import { childrenOf, useChatsHere, useChatsSelect } from "./chatState";
import type { ListedChat } from "./chatsTree";
import {
  countTasks,
  countsSaid,
  firstNeeding,
  helperShown,
  helpersSaid,
  sameCounts,
  tasksSaid,
} from "./explorerTasks";
import { StateShown } from "./StateShown";

/*
 * **What the explorer draws of a chat's tasks and helpers** (#1490): the count of helpers on a
 * chat's row and the rows it unfolds to, the one line for a session's tasks, and the hand a
 * session's row wears for a task that needs you.
 *
 * **Each reads its own share of what the chats are doing** (SC-3), as a row's state mark does:
 * a task moving redraws the one line's counts, a helper moving its own row, and the tree
 * around them is not drawn again. What each says is `explorerTasks.ts`'s.
 */

/** What a row says of its place in the tree: `Explorer`'s own (`TreeItem`), as these rows
 *  are handed it. */
type Item = {
  role: "treeitem";
  "aria-level"?: number;
  "aria-posinset"?: number;
  "aria-setsize"?: number;
  "aria-expanded"?: boolean;
  "data-row": string;
};

/** The id of chat `session`'s count of helpers, which the chat's row is described by: an id
 *  that names no element, for a chat with none, describes it with nothing. */
export function helpersId(session: number): string {
  return `helpers-of-${session}`;
}

/** The id of the hand chat `session`'s row wears for a task below it, which the row is
 *  described by while it wears one. */
export function tasksNeedId(session: number): string {
  return `tasks-need-${session}`;
}

/**
 * **A chat's helpers, as a count on its row** (V100-4): `3 helpers`, which a press unfolds to
 * a row each and folds again. A row of the tree, the chat's first child, so the arrows reach
 * it and a screen reader hears how many and whether they are unfolded; drawn on the chat's own
 * line, since it is a fact about that chat and not a place of its own.
 *
 * Reads how many off the project's store itself, so helpers coming and going change this text
 * and not the tree.
 */
export const HelpersCount = memo(function HelpersCount({
  session,
  name,
  open,
  focusable,
  item,
  onFold,
}: {
  session: number;
  /** The chat's name, which the count is said to be of. */
  name: string;
  open: boolean;
  /** Whether the arrows stop on it: not inside a folded repo. */
  focusable: boolean;
  item: Item;
  onFold: (session: number, open: boolean) => void;
}) {
  const count = useChatsSelect(useChatsHere(), (states) => childrenOf(states, session).length);
  if (count === 0) return null;
  return (
    <RovingFocusGroup.Item asChild tabStopId={item["data-row"]} focusable={focusable}>
      <button
        type="button"
        id={helpersId(session)}
        className="helpers-count"
        {...item}
        title={open ? `Fold the helpers of ${name}` : `Show the helpers of ${name}`}
        onClick={() => onFold(session, !open)}
      >
        {/* The space is outside the hidden words, so the name reads as words. */}
        {helpersSaid(count)} <span className="hidden-words">of {name}</span>
      </button>
    </RovingFocusGroup.Item>
  );
});

/**
 * **One helper, unfolded**: `helper <short id>` and its state as its harness reports it, in
 * the words every row uses. A row of the tree that does nothing when pressed: a helper is not
 * something to bring forward or start in. Its asks are its chat's, and a stop of the chat
 * stops it. The whole id is its tooltip.
 *
 * Reads its own state off the store, so a helper moving redraws this row alone.
 */
export const HelperRow = memo(function HelperRow({
  session,
  agent,
  shown,
  focusable,
  item,
}: {
  session: number;
  /** The harness's id for it, whole. */
  agent: string;
  /** Its short id (`shortIds`). */
  shown: string;
  focusable: boolean;
  item: Item;
}) {
  const state = useChatsSelect(
    useChatsHere(),
    (states) => childrenOf(states, session).find((child) => child.agent === agent)?.state,
  );
  return (
    <RovingFocusGroup.Item asChild tabStopId={item["data-row"]} focusable={focusable}>
      <div className="chat helper" title={agent} {...item}>
        <span className="session">helper {shown}</span>{" "}
        {state !== undefined && <StateShown shown={helperShown(state)} />}
      </div>
    </RovingFocusGroup.Item>
  );
});

/** Stable nothing, for a line with no finished tasks. */
const NONE_ENDED: readonly FinishedTask[] = [];

/**
 * **How many tasks, and how many in each state** (V100-14): `5 tasks 2 working 3 done`, and
 * `1 failed` and `1 needs you` when there are any. The words are the rows' own.
 *
 * The only thing a task moving draws again (SC-3): it reads the tasks' states itself, and is
 * drawn again only when a count changes.
 */
const TasksSummary = memo(function TasksSummary({
  tasks,
  ended,
  where,
}: {
  tasks: readonly ListedChat[];
  ended: readonly FinishedTask[];
  /** What is said after how many: where they came from, for the tasks other workspaces'
   *  chats asked for. */
  where?: string;
}) {
  const counts = useChatsSelect(
    useChatsHere(),
    (states) => countTasks(states, tasks, ended),
    sameCounts,
  );
  return (
    <>
      <span className="session">
        {tasksSaid(counts.total)}
        {where !== undefined && ` ${where}`}
      </span>
      {countsSaid(counts).map(({ state, said }) => (
        // A space before each, so the line reads as words; the row's flex drops it.
        <span key={state} className="count" data-state={state}>
          {" "}
          {said}
        </span>
      ))}
    </>
  );
});

/**
 * **The one line for a set of tasks** (V100-14): a session's, under its row, or the tasks
 * working here that chats of other workspaces asked for, under the workspace. A row of the
 * tree. Pressing it opens no chat: it goes to the Chats list, which is where tasks are listed.
 */
export function TasksLine({
  tasks,
  ended = NONE_ENDED,
  where,
  title,
  focusable,
  item,
  onPress,
}: {
  /** The running tasks counted. Held by whoever draws the line, so the counts are read again
   *  only when the tasks or the board change. */
  tasks: readonly ListedChat[];
  ended?: readonly FinishedTask[];
  where?: string;
  /** What a press does, in words. */
  title: string;
  focusable: boolean;
  item: Item;
  onPress: () => void;
}) {
  return (
    <RovingFocusGroup.Item asChild tabStopId={item["data-row"]} focusable={focusable}>
      <button type="button" className="chat tasks-line" title={title} {...item} onClick={onPress}>
        <ListTree className="node-icon" aria-hidden="true" />
        <TasksSummary tasks={tasks} ended={ended} where={where} />
      </button>
    </RovingFocusGroup.Item>
  );
}

/**
 * **The hand a session's row wears for a task of it that needs you** (#1448's roll-up, in the
 * explorer): the task has no row here, so its session's row answers for it, as a folded row of
 * the Chats list does. A button beside the row, since a press goes to the task and not to the
 * session; out of the arrows' way, and the row is described by it, so a screen reader on the
 * row hears it.
 *
 * Reads the queue itself and draws nothing while no task of the session is in it.
 */
export const TasksNeedYou = memo(function TasksNeedYou({
  session,
  name,
  tasks,
  onShow,
}: {
  session: number;
  name: string;
  tasks: readonly ListedChat[];
  onShow: (session: number) => void;
}) {
  const needs = useChatsSelect(useChatsHere(), (states) => firstNeeding(states.needsYou, tasks));
  if (needs === undefined) return null;
  const task = tasks.find((one) => one.session === needs)?.name ?? "a task";
  const said = `Go to ${task}, a task of ${name}, which needs you`;
  return (
    <button
      type="button"
      id={tasksNeedId(session)}
      className="needs-you-mark rolled-up"
      data-mark="needs-you"
      data-leads-to={needs}
      tabIndex={-1}
      aria-label={said}
      title={said}
      onClick={() => onShow(needs)}
    >
      <Hand aria-hidden="true" />
    </button>
  );
});
