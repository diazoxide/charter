import type { OpenChat, Sidebar as SidebarModel } from "./bindings";
import { ChatState } from "./NeedsYou";
import { type ChatStates, stateOf } from "./chatState";

/** The left-hand side: every workspace with its chats, and the focused one's todos.
 *
 *  A chat here is the app's own — `.charter/frame/` belongs to the tmux frame and the app
 *  stays out of it (`docs/plane-format.md`), so nothing on the plane records a chat. What
 *  ties one to a workspace is the directory it works in, and a chat working outside every
 *  workspace is shown rather than dropped. */
export function Sidebar({
  sidebar,
  states,
  focused,
  onFocus,
}: {
  sidebar: SidebarModel;
  /** What each chat is doing, from its harness's own hooks (spec decision 3). */
  states: ChatStates;
  focused: string | undefined;
  onFocus: (workspace: string) => void;
}) {
  const inFront = sidebar.workspaces.find((ws) => ws.name === focused);
  return (
    <nav className="sidebar" aria-label="Workspaces">
      <ul className="workspaces" role="tablist" aria-label="Workspaces" aria-orientation="vertical">
        {sidebar.workspaces.map((ws) => (
          <li className="workspace" key={ws.name} data-testid={`workspace-${ws.name}`}>
            <button role="tab" aria-selected={ws.name === focused} onClick={() => onFocus(ws.name)}>
              {ws.name}
            </button>
            <p className="vision">{ws.vision || "No vision yet"}</p>
            {ws.chats.length === 0 ? (
              <p className="chats none">No chats</p>
            ) : (
              <ul className="chats">
                {ws.chats.map((chat) => (
                  <li key={chat.session}>
                    <ChatRow chat={chat} states={states} />
                  </li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>

      {sidebar.unfiled.length > 0 && (
        <section className="unfiled" data-testid="unfiled">
          <h2>Outside every workspace</h2>
          <ul>
            {sidebar.unfiled.map((chat) => (
              <li key={chat.session}>
                <ChatRow chat={chat} states={states} />
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="focused" data-testid="focused">
        <h2>{inFront ? inFront.name : "No workspace"}</h2>
        <p className="persona">Persona: {sidebar.persona ?? "none declared"}</p>
        {inFront && inFront.todos.length > 0 ? (
          <ul className="todos">
            {inFront.todos.map((todo) => (
              <li key={todo}>{todo}</li>
            ))}
          </ul>
        ) : (
          <p className="todos none">Nothing to do</p>
        )}
      </section>
    </nav>
  );
}

/** One chat: what it is called, what it runs, and where it is working.
 *
 *  The same `OpenChat` the quit warning lists and the record brings back — one model of a
 *  chat, not a second derived from the sessions. */
function ChatRow({ chat, states }: { chat: OpenChat; states: ChatStates }) {
  return (
    <>
      <span className="session">{chat.name}</span>
      {/* What it is doing, from its own harness's hooks. A chat whose harness reports none
          reads `unknown`, which is what the spec says it should (decision 3) — the word is
          the mark's accessible name, so it is never colour alone. */}
      <ChatState state={stateOf(states, chat.session)} />
      {/* The PROFILE where there is one, and the harness otherwise. A profile is what the
          operator picked and what a relaunch looks up again; the kind is what the plane
          calls the harness. Showing the profile alone would hide which harness it runs, and
          showing the kind alone would hide which account. */}
      {chat.profile ? (
        <span className="harness">
          {" · "}
          {chat.profile}
          {chat.harness && <span className="kind"> ({chat.harness})</span>}
        </span>
      ) : (
        chat.harness && <span className="harness"> · {chat.harness}</span>
      )}
      {chat.persona && <span className="persona"> · {chat.persona}</span>}
      {chat.cwd && <code className="cwd">{chat.cwd}</code>}
    </>
  );
}
