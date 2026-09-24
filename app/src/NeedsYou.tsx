/** What a chat is doing, as the tab and the queue draw it. */
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { CircleCheck, Hand, SquareTerminal } from "lucide-react";
import { type State } from "./chatState";
import { useArrived } from "./lib/arrived";
import { useTabStop } from "./roving";

/** The word beside a chat's name. */
const WORDS: Record<State, string> = {
  unknown: "unknown",
  running: "running",
  waiting: "waiting on you",
  done: "done",
  failed: "failed",
};

export function ChatState({ state }: { state: State }) {
  // A state this mark CHANGED to, rather than the one it was drawn in: only the first moves,
  // so a workspace's tabs coming back into view do not all pulse at once (`useArrived`).
  const arrived = useArrived(state);
  return (
    <span
      className={`state state-${state}${arrived ? " arrived" : ""}`}
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
 * Codex chat stopped mid-turn for an approval says nothing (charter-app#52, and there is no
 * way for it to say so that does not arm a hook which DECIDES a permission) — can be waiting
 * on you while the queue is empty.
 *
 * **So the empty state says what is known and not more.** "Nothing needs you" is a claim
 * about every chat on the plane; while a chat that cannot report one is open, what charter
 * actually knows is that nothing has SAID so. The hedge used to sit in a second sentence
 * under a headline that still claimed the certainty — a precise "this may be waiting and
 * cannot say" beats a confident wrong state, and it cannot be built out of two sentences
 * that disagree.
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
  // The queue is ONE Tab stop, its oldest chat, and Up and Down move along it (charter-app#189,
  // `roving.ts`): at fifty chats asking, fifty stops would be the strip's problem over again.
  const stop = useTabStop(undefined, queue.map(String));
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
    // not tell anyone the app is watching. Which of the two sentences it is depends on
    // whether every open chat can report — never both, because they are different claims.
    return (
      <div className="needs-you needs-you-empty" aria-label="Needs you">
        <span>
          <CircleCheck className="node-icon" />
          {quiet.length === 0 ? "Nothing needs you" : "Nothing has said it needs you"}
        </span>
        {unsaid}
      </div>
    );
  }
  return (
    // **The loudest thing in the window, and deliberately.** This number is why charter-app
    // exists: it is how many chats have stopped and are waiting on the operator, and at fifty
    // sessions it is the only thing on screen that is worth interrupting for. So it is drawn
    // at a size nothing else here has, in `needs-you.base`, with the one hand-raised mark the
    // window uses — and the sentence still says the number in words for anyone who is read to.
    <div className="needs-you" aria-label="Needs you">
      <span className="needs-you-count">
        <Hand className="node-icon" />
        <strong className="needs-you-number">{queue.length}</strong>
        {" need you"}
      </span>
      <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
        <ul>
          {queue.map((session) => (
            <li key={session}>
              <RovingFocusGroup.Item asChild tabStopId={String(session)}>
                <button onClick={() => show(session)}>
                  <SquareTerminal className="node-icon" />
                  {nameOf(session)}
                </button>
              </RovingFocusGroup.Item>
            </li>
          ))}
        </ul>
      </RovingFocusGroup.Root>
      {unsaid}
    </div>
  );
}
