import { memo, type ReactNode } from "react";
import { SquareTerminal } from "lucide-react";
import { ChatMark, WrappingUp } from "./NeedsYou";
import {
  childrenOf,
  markOf,
  sameChildren,
  useChatsHere,
  useChatsSelect,
  type State,
} from "./chatState";
import { PlaneUpdatedMark, type PlaneUpdates } from "./PlaneUpdated";
import { chatOf, contentsOf, panesOf, type Tabs } from "./tabs";
import { ViewMark } from "./Views";

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

/** How many characters of a harness's id for a child agent its row shows: enough to tell
 *  apart two ids that differ late (`thread-1`, `thread-10`). The whole id is its tooltip. */
const AGENT_ID_SHOWN = 16;

/** A child agent's id as its row shows it: whole when it fits, cut with an ellipsis when not. */
function shownId(agent: string): string {
  return agent.length > AGENT_ID_SHOWN ? `${agent.slice(0, AGENT_ID_SHOWN)}…` : agent;
}

/** The id of chat `session`'s list of sub-agents, which the chat's row is described by. */
export function childAgentsId(session: number): string {
  return `sub-agents-of-${session}`;
}

/** The words a child agent's state can be, as `ChatMark` draws them. */
const CHILD_STATES: readonly string[] = ["running", "waiting", "done", "failed"];

/**
 * **A chat's child agents, under its row** (FD-18, W8): each sub-agent or child its harness
 * spawned, by the harness's id for it, with what it is doing. Reads its own chat's children off
 * the project's store, as the state mark does (SC-3), and draws nothing for a chat with none.
 *
 * Not rows of the tree: a child is not something to bring forward or start in, so the arrows
 * stop on its chat and not on it. Its asks are its chat's, and a stop of the chat stops it.
 */
export const ChildAgents = memo(function ChildAgents({
  session,
  name,
}: {
  session: number;
  /** The chat's name, which the list is labelled by. */
  name: string;
}) {
  const children = useChatsSelect(
    useChatsHere(),
    (states) => childrenOf(states, session),
    sameChildren,
  );
  if (children.length === 0) return null;
  return (
    <ul
      id={childAgentsId(session)}
      className="child-agents"
      role="list"
      aria-label={`Sub-agents of ${name}`}
    >
      {children.map((child) => (
        <li key={child.agent} className="child-agent">
          <span className="agent" title={child.agent}>
            sub-agent {shownId(child.agent)}
          </span>
          <ChatMark
            state={CHILD_STATES.includes(child.state) ? (child.state as State) : "unknown"}
          />
        </li>
      ))}
    </ul>
  );
});

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
}: {
  tabs: Tabs;
  id: number;
  /** The chats wrapping up — being smart-closed (ADR 0064). */
  wrapping: ReadonlySet<number>;
  /** The chats the plane's instructions changed under, by session (charter#369). */
  updates: PlaneUpdates;
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
          read before the name is. A harness chat wears none — it is the ordinary case. */}
      {chat !== undefined && shells.has(chat) && (
        <SquareTerminal className="tab-mark" data-mark="shell" aria-hidden="true" />
      )}
      <span className="tab-name">{tabs.byId[id].name}</span>
      {pin}
      <PlaneUpdatedMark files={chat === undefined ? undefined : updates[chat]} />
      {/* The first pane's session is the tab's own chat. Its own element, so what a tab IS
          stays separate from what it is DOING — a tab whose text changed every time a turn
          began would be unreadable, and untestable. */}
      <ChatStateMark session={chat} shell={chat !== undefined && shells.has(chat)} />
      <WrappingUp held={panesOf(tabs, id).some((one) => wrapping.has(one.session))} />
    </>
  );
}
