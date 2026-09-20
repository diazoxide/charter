/** What a chat is doing, as the tab and the queue draw it. */
import { type State } from "./chatState";

/** The word beside a chat's name. */
const WORDS: Record<State, string> = {
  unknown: "unknown",
  running: "running",
  waiting: "waiting on you",
  done: "done",
  failed: "failed",
};

export function ChatState({ state }: { state: State }) {
  return (
    <span
      className={`state state-${state}`}
      data-state={state}
      // The word, never only a colour: a state told apart by colour alone is no state at all
      // to anyone who cannot see it, and `title` alone is no use on a touch screen or to a
      // screen reader reading a list. The mark is decorative; the label carries the meaning.
      role="img"
      aria-label={WORDS[state]}
      title={WORDS[state]}
    />
  );
}

/**
 * The chats asking for you, across every workspace.
 *
 * It holds only chats that ASKED — one that has never run is `waiting` for a first prompt,
 * which is not the same thing. A relaunch that put twenty chats back must not fill this.
 *
 * And it names the chats it cannot vouch for. A harness that cannot report everything — a
 * Codex chat stopped mid-turn for an approval says nothing — can be waiting on you while
 * the queue is empty, so "Nothing needs you" is never said over the top of it alone.
 */
export function NeedsYou({
  queue,
  quiet,
  nameOf,
  show,
}: {
  queue: readonly number[];
  /** The chats that can be waiting on you without saying so, by name. */
  quiet: readonly string[];
  nameOf: (session: number) => string;
  show: (session: number) => void;
}) {
  const unsaid =
    quiet.length === 0 ? null : (
      <span className="needs-you-quiet">
        {quiet.length === 1
          ? `${quiet[0]} can be waiting on you without saying so.`
          : `${quiet.length} chats can be waiting on you without saying so.`}
      </span>
    );
  if (queue.length === 0) {
    // Said rather than left blank: an empty queue is the good state, and a blank space does
    // not tell anyone the app is watching.
    return (
      <div className="needs-you needs-you-empty" aria-label="Needs you">
        <span>Nothing needs you</span>
        {unsaid}
      </div>
    );
  }
  return (
    <div className="needs-you" aria-label="Needs you">
      <span className="needs-you-count">{queue.length} need you</span>
      <ul>
        {queue.map((session) => (
          <li key={session}>
            <button onClick={() => show(session)}>{nameOf(session)}</button>
          </li>
        ))}
      </ul>
      {unsaid}
    </div>
  );
}
