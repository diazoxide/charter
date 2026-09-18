import type { OpenChat } from "./bindings";

/**
 * What quitting asks before it ends anything.
 *
 * It lists the sessions that are about to end, and says plainly that charter cannot tell
 * whether one is mid-turn. It could not: a session's state comes from harness hooks only
 * (spec decision 3), and the app has none yet. The alternative — calling a session "busy"
 * because bytes arrived recently — would be the app guessing at a harness's turn from its
 * output, which is the one thing it never does.
 */
export function QuitWarning({
  chats,
  onQuit,
  onCancel,
}: {
  chats: OpenChat[];
  onQuit: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="asking">
      <div className="warning" role="dialog" aria-modal="true" aria-labelledby="quit-warning">
        <h2 id="quit-warning">
          {chats.length === 1
            ? "1 session will be ended"
            : `${chats.length} sessions will be ended`}
        </h2>
        <ul className="ending">
          {chats.map((chat) => (
            <li key={chat.session}>
              <span className="what">{chat.harness ?? "shell"}</span>
              <span className="who">{chat.name}</span>
              {chat.cwd && <code className="where">{chat.cwd}</code>}
            </li>
          ))}
        </ul>
        <p className="honest">
          charter cannot yet tell whether a session is mid-turn — that arrives with hook-driven
          session state.
        </p>
        <div className="answer">
          {/* Cancel first, and focused: the destructive answer is never the one a stray
              Return key finds. */}
          <button autoFocus onClick={onCancel}>
            Cancel
          </button>
          <button className="ends-it" onClick={onQuit}>
            Quit charter
          </button>
        </div>
      </div>
    </div>
  );
}
