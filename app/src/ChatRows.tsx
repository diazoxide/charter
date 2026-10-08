import { memo, type ReactNode } from "react";
import { Hand, ListTodo, SquareTerminal } from "lucide-react";
import { ChatMark, WrappingUp } from "./NeedsYou";
import { markOf, useChatsHere, useChatsSelect } from "./chatState";
import { PlaneUpdatedMark, type PlaneUpdates } from "./PlaneUpdated";
import { chatOf, contentsOf, panesOf, type Tabs } from "./tabs";
import { PersonaMark } from "./PersonaMark";
import { ViewMark } from "./Views";
import { shownState, type Shown, type TaskFacts } from "./shownState";
import { StateShown } from "./StateShown";
import { needsSaid } from "./tabTasks";
import { tasksAtWorkOf } from "./sessionTasks";
import { useTasksBelow } from "./TasksBelow";

/*
 * **The rows that draw a chat** (SC-3): split out of `PlaneView`, whose rendering they no longer
 * share. Each reads its own chat's state off the project's store, so a chat moving redraws its
 * rows and not the project view around them.
 */

/**
 * **One chat's state mark, which reads that chat's state itself** (SC-3).
 *
 * Every row that draws a chat — its tab on the strip, its line in the show-more menu, its row in
 * the explorer — draws this, and this is all of the row that a move changes. Subscribed to its
 * own chat and nothing else, so a move redraws the moved chat's marks and no other row; held
 * (`memo`), so a row redrawn for its own reasons does not redraw it.
 */
export const ChatStateMark = memo(function ChatStateMark({
  session,
  shell,
}: {
  /** The chat, or none: a tab with no chat in it draws `unknown`, as `markOf` says. */
  session: number | undefined;
  /** Whether it is a shell tab, which draws no mark until something reports a state for it. */
  shell: boolean;
}) {
  const state = useChatsSelect(useChatsHere(), (states) => markOf(states, session ?? -1, shell));
  return <ChatMark state={state} />;
});

/** Whether two shown states say the same thing. */
function sameShown(one: Shown | undefined, other: Shown | undefined): boolean {
  return one === other || (one?.kind === other?.kind && one?.word === other?.word);
}

/**
 * **One chat's state as a row says it, a mark and a word, which reads that chat's state
 * itself** (#1484, SC-3). The Chats list's row and the explorer's both draw this, from the same
 * facts through the one function (`shownState`), so they say the same of a chat.
 *
 * Subscribed to its own chat and to the queue's answer about it, and redrawn only when what it
 * shows changes: a move redraws the moved chat's state and no row. Held on plain values, so a
 * row redrawn for its own reasons does not redraw it.
 *
 * **A chat waiting on its tasks says so, and how many** (#1491): it reads its own tasks
 * (`useTasksBelow`) and counts the ones that have not finished, by the one count
 * (`sessionTasks.tasksAtWorkOf`), so the word agrees with the count on its row. A task below it
 * that finishes redraws this, and no other chat's.
 */
export const ChatShownState = memo(function ChatShownState({
  session,
  shell,
  report = null,
  outcome = null,
  asking = null,
  harness,
  changed = false,
}: {
  session: number;
  /** Its state changed while its row was not drawn, so it arrives in it (`StateShown`). */
  changed?: boolean;
  /** A shell tab, which shows no state until something reports one. */
  shell: boolean;
  /** As a task, the report it owes (`TaskFacts`); nothing for a chat that is not one. */
  report?: TaskFacts["report"] | null;
  /** How it reported, where it has and the app knows. */
  outcome?: string | null;
  /** The chat it has a question open with, by name. */
  asking?: string | null;
  /** Its harness as the person calls it: named in what is not known of it. */
  harness: string | null;
}) {
  const below = useTasksBelow(session);
  const shown = useChatsSelect(
    useChatsHere(),
    (states) =>
      shownState({
        board: markOf(states, session, shell),
        needsYou: states.needsYou.includes(session),
        task: report === null ? null : { report, outcome, asking },
        harness,
        tasksAtWork: tasksAtWorkOf(states, below),
      }),
    sameShown,
  );
  return shown === undefined ? null : <StateShown shown={shown} changed={changed} />;
});

/** No names: what a tab with no hidden task that needs you is handed. */
const NO_NAMES: readonly string[] = [];

/**
 * What a tab says about itself, on the strip and in the menu of what the strip has no room for.
 *
 * **A chat's tab is its name and what it is doing; a view's tab is a mark and its name**, with
 * no state — a view is not doing anything, and a dot beside it would be a claim about a chat
 * the tab does not have. The mark says what kind of thing the tab holds before the name is
 * read: a person for a persona, a puzzle piece for a view an extension offers.
 */
export function TabMarks({
  tabs,
  id,
  updates,
  shells,
  pin,
  wrapping,
  persona,
  task,
  taskOf,
  needs = NO_NAMES,
}: {
  tabs: Tabs;
  id: number;
  /**
   * **The tab is a task's own** (#1489, V100-38): the name of the chat that asked for it, or
   * nothing where that chat has no name to say any more. The tab then wears the task mark
   * before its name and says whose task it is after it, dimmer (`talk · steward 4`), so it is
   * never read as a session's tab. Left out for every other tab.
   */
  taskOf?: { asker: string | undefined };
  /** The task the tab shows in place of its session's own chat, by name (#1486): the label
   *  says it after the session's name, dimmer. Nothing while the tab shows its own chat. */
  task?: string;
  /** The tab's chats that need you and are not on screen in it, by name, longest waiting
   *  first (#1486): the hand, where the tab is a row of the menu of what the strip has no
   *  room for. **Left out on the strip** (#1487): there the hand is on the tab's chip, beside
   *  the tab, and is pressed (`TabChip.tsx`). */
  needs?: readonly string[];
  /** The persona the tab's chat runs as, when it runs as one: its mark is the tab's. */
  persona?: string | null;
  /** The chats wrapping up — being smart-closed (ADR 0064). */
  wrapping: ReadonlySet<number>;
  /** The chats the plane's instructions changed under, by session (charter#369). Left out on the
   *  strip, where the mark is a button beside the tab (`FreshMark`) rather than inside it. */
  updates?: PlaneUpdates;
  /** The chats that are shell tabs, whose tab wears a terminal's mark (SI-5). */
  shells: ReadonlySet<number>;
  /** The pin mark, on the strip; the menu of hidden tabs draws none. */
  pin?: ReactNode;
}) {
  const lead = contentsOf(tabs, id)[0]?.content;
  const chat = chatOf(tabs, id);
  if (lead?.kind === "view") {
    return (
      <>
        <ViewMark view={lead.view} />
        {/* The preview tab is in italics, as VS Code draws its own: the tab the next single
            click on a memory reuses (SI-9b). */}
        <span className={lead.preview ? "tab-name is-preview" : "tab-name"}>
          {tabs.byId[id].name}
        </span>
        {pin}
      </>
    );
  }
  return (
    <>
      {/* A shell tab's mark, before its name as a view's is: what kind of thing the tab holds,
          read before the name is. A chat that runs as a persona wears that persona's (#1449);
          one that runs as none wears nothing. */}
      {taskOf !== undefined ? (
        /* The task mark: what kind of thing the tab holds, read before its name is (#1489). */
        <ListTodo className="tab-mark" data-mark="task" aria-hidden="true" />
      ) : chat !== undefined && shells.has(chat) ? (
        <SquareTerminal className="tab-mark" data-mark="shell" aria-hidden="true" />
      ) : (
        persona != null && <PersonaMark persona={persona} className="tab-mark" />
      )}
      <span className="tab-name">{tabs.byId[id].name}</span>
      {pin}
      <PlaneUpdatedMark files={chat === undefined ? undefined : updates?.[chat]} />
      {/* The first pane's session is the tab's own chat. Its own element, so what a tab IS
          stays separate from what it is DOING — a tab whose text changed every time a turn
          began would be unreadable, and untestable.

          **Before the task's name, where the tab shows a task** (#1486): the mark is the
          session's, and beside the session's name it reads as the session's. */}
      <ChatStateMark session={chat} shell={chat !== undefined && shells.has(chat)} />
      {/* **`talk · steward 4`, on a task's own tab** (#1489): whose task it is, dimmer. To a
          screen reader the separator is the words it stands for. */}
      {taskOf !== undefined && (
        <>
          <span className="hidden-words">, a task{taskOf.asker === undefined ? "" : " of "}</span>
          {taskOf.asker !== undefined && (
            <>
              <span className="tab-task-sep" aria-hidden="true">
                {" · "}
              </span>
              <span className="tab-task tab-asker">{taskOf.asker}</span>
            </>
          )}
        </>
      )}
      {/* **`steward 4 › talk`, while the tab shows a task** (#1486, V100-35). The tab is the
          session's, so the session's name stays first and the task's is the dimmer one. To a
          screen reader the separator is the words it stands for. */}
      {task !== undefined && (
        <>
          <span className="tab-task-sep" aria-hidden="true">
            {" › "}
          </span>
          <span className="hidden-words">, showing task </span>
          <span className="tab-task">{task}</span>
        </>
      )}
      {/* **A chat of this tab is waiting for you and is not on screen** (#1486, V100-37): the
          hand, as a row of the Chats list wears it. A mark and no button here: this is inside
          a button, the tab's own or a row of the show-more menu. On the strip the hand is the
          chip's and goes to that chat (#1487); from the menu the way is to bring the tab
          forward, and its chip is there. */}
      {needs.length > 0 && (
        <span
          className="needs-you-mark"
          data-mark="needs-you"
          role="img"
          aria-label={needsSaid(needs)}
          title={needsSaid(needs)}
        >
          <Hand aria-hidden="true" />
        </span>
      )}
      <WrappingUp held={panesOf(tabs, id).some((one) => wrapping.has(one.session))} />
    </>
  );
}
