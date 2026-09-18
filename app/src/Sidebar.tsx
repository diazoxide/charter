import type { OpenChat, Sidebar as SidebarModel } from "./bindings";

/** The left-hand side: every workspace with its chats, and the focused one's todos.
 *
 *  A chat here is the app's own — `.charter/frame/` belongs to the tmux frame and the app
 *  stays out of it (`docs/plane-format.md`), so nothing on the plane records a chat. What
 *  ties one to a workspace is the directory it works in, and a chat working outside every
 *  workspace is shown rather than dropped. */
export function Sidebar({
  sidebar,
  focused,
  onFocus,
}: {
  sidebar: SidebarModel;
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
                    <ChatRow chat={chat} />
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
                <ChatRow chat={chat} />
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
function ChatRow({ chat }: { chat: OpenChat }) {
  return (
    <>
      <span className="session">{chat.name}</span>
      {chat.harness && <span className="harness"> · {chat.harness}</span>}
      {chat.cwd && <code className="cwd">{chat.cwd}</code>}
    </>
  );
}
