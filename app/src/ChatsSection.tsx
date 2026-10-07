import { memo } from "react";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { MessagesSquare, SquareTerminal } from "lucide-react";
import { ChatStateMark, NeedsYouMark } from "./ChatRows";
import type { ChatRow } from "./chatsTree";
import { PersonaMark } from "./PersonaMarkStandIn";
import { useTabStop } from "./roving";

/** A row's id in the section's roving focus. */
const rowId = (session: number) => `chats:${session}`;

/**
 * **Every running chat of the project, in one tree** (#1447), in the left region above the
 * explorer. The explorer answers what is running in this workspace; this answers who is doing
 * what across all of them, and which chat started which.
 *
 * **A row is a way to the chat.** Pressing one brings its tab forward, on whichever workspace's
 * strip it is. A task chat has no tab: its row says so, and pressing it opens an ordinary tab.
 *
 * **A chat moving redraws its own marks and no row** (SC-3). The rows are held, on plain
 * values, and each mark reads its own chat's state, so fifty chats cost one mark per move.
 */
export function ChatsSection({
  rows,
  front,
  onOpen,
}: {
  rows: readonly ChatRow[];
  /** The chat in front, whose row is the current one. */
  front?: number;
  /** A row was pressed: bring that chat forward, opening its tab when it has none. */
  onOpen: (session: number) => void;
}) {
  const stop = useTabStop(
    front === undefined ? undefined : rowId(front),
    rows.map((row) => rowId(row.session)),
  );
  return (
    <section className="chats-section" data-testid="chats-section" aria-labelledby="chats-title">
      <h2 className="sidebar-title" id="chats-title">
        <MessagesSquare className="node-icon" aria-hidden="true" />
        Chats
      </h2>
      {rows.length === 0 ? (
        <p className="empty">No chats are running in this project.</p>
      ) : (
        <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
          <ul role="tree" aria-label="Chats of this project">
            {rows.map((row) => (
              <Row
                key={row.session}
                session={row.session}
                name={row.name}
                persona={row.persona}
                workspace={row.workspace}
                shell={row.shell}
                level={row.level}
                posinset={row.posinset}
                setsize={row.setsize}
                from={row.orphaned ? row.from : null}
                tab={row.tab}
                current={row.session === front}
                onOpen={onOpen}
              />
            ))}
          </ul>
        </RovingFocusGroup.Root>
      )}
    </section>
  );
}

/** One chat's row. Held on plain values, so only a row whose own facts changed is drawn again. */
const Row = memo(function Row({
  session,
  name,
  persona,
  workspace,
  shell,
  level,
  posinset,
  setsize,
  from,
  tab,
  current,
  onOpen,
}: {
  session: number;
  name: string;
  persona: string | null;
  workspace: string;
  shell: boolean;
  level: number;
  posinset: number;
  setsize: number;
  /** The closed chat it came from, by name; nothing for a chat drawn under its parent. */
  from: string | null;
  tab: boolean;
  current: boolean;
  onOpen: (session: number) => void;
}) {
  return (
    <li role="none" data-level={level}>
      <RovingFocusGroup.Item asChild tabStopId={rowId(session)}>
        <button
          type="button"
          className="chat"
          role="treeitem"
          aria-level={level}
          aria-posinset={posinset}
          aria-setsize={setsize}
          aria-current={current || undefined}
          data-tab={tab}
          title={tab ? undefined : "No tab yet. Press to open it as a tab."}
          onClick={() => onOpen(session)}
        >
          {persona === null ? (
            <SquareTerminal className="node-icon" aria-hidden="true" />
          ) : (
            <PersonaMark persona={persona} />
          )}
          <span className="session">{name}</span>
          <span className="workspace">{workspace}</span>
          <ChatStateMark session={session} shell={shell} />
          <NeedsYouMark session={session} />
          {from !== null && <span className="from">from {from}</span>}
          {!tab && <span className="no-tab">no tab</span>}
        </button>
      </RovingFocusGroup.Item>
    </li>
  );
});
