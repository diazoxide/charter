import type { OpenChat } from "./bindings";
import { ChatState } from "./NeedsYou";
import { type ChatStates, stateOf } from "./chatState";

/**
 * What quitting asks before it ends anything.
 *
 * It lists the sessions that are about to end and what each one is doing. Until M1.3 it said
 * charter could not tell whether a session was mid-turn; now a harness's own hooks say so
 * (spec decision 3), and the ones that still cannot are named rather than lumped in with the
 * rest. Nothing here is guessed from a session's output, which is the one thing the app never
 * does (ADR 0018).
 */
export function QuitWarning({
  chats,
  states,
  onQuit,
  onCancel,
}: {
  chats: OpenChat[];
  states: ChatStates;
  onQuit: () => void;
  onCancel: () => void;
}) {
  const running = chats.filter((chat) => stateOf(states, chat.session) === "running");
  const unknown = chats.filter((chat) => stateOf(states, chat.session) === "unknown");
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
              <ChatState state={stateOf(states, chat.session)} />
              {chat.cwd && <code className="where">{chat.cwd}</code>}
            </li>
          ))}
        </ul>
        {running.length > 0 && (
          <p className="honest mid-turn" role="alert">
            {running.length === 1
              ? `${running[0].name} is mid-turn and will be interrupted.`
              : `${running.length} sessions are mid-turn and will be interrupted.`}
          </p>
        )}
        {unknown.length > 0 && (
          <p className="honest">
            {/* Named, not counted into the reassuring number. A harness that reports nothing
                could be mid-turn and charter would never know — saying "nothing is running"
                over the top of it would be the app claiming something it cannot see. */}
            {unknown.length === 1
              ? `${unknown[0].name} reports no state, so charter cannot tell whether it is mid-turn.`
              : `${unknown.length} sessions report no state, so charter cannot tell whether they are mid-turn.`}
          </p>
        )}
        {running.length === 0 && unknown.length === 0 && (
          <p className="honest">No session is mid-turn.</p>
        )}
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
