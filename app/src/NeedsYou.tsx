/** What a chat is doing, as the tab and the queue draw it. */
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { CircleCheck, Hand, MessageSquareReply, SquareTerminal, X } from "lucide-react";
import type { KeyboardEvent } from "react";
import { ignoreId, type Catalogued, type Offer } from "./actions";
import { type State } from "./chatState";
import { useArrived } from "./lib/arrived";
import { useTabStop } from "./roving";
import { deletes } from "./tabKeys";

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
  offers,
  onPress,
  reportsTo = () => [],
}: {
  queue: readonly number[];
  /** The chats that can be waiting on you without saying so, by name. */
  quiet: readonly string[];
  nameOf: (session: number) => string;
  show: (session: number) => void;
  /**
   * The chats that have reported back to a chat in the queue, by name (charter-app#259). Such
   * an item says `<child> reported back` — what the operator is being asked to look at — and
   * Go still opens the chat that asked, whose next turn is handed the report.
   */
  reportsTo?: (session: number) => readonly string[];
  /** The catalogue by id, where each item's Ignore row (`needs.ignore:<session>`) is. */
  offers: Catalogued;
  onPress: (offer: Offer) => void;
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
          {queue.map((session) => {
            const ignore = offers.get(ignoreId(session));
            const reported = reportsTo(session);
            return (
              <li key={session}>
                <RovingFocusGroup.Item asChild tabStopId={String(session)}>
                  <button
                    onClick={() => show(session)}
                    onKeyDown={(event) => ignoreOnDelete(event, ignore, onPress)}
                    // Where Go goes, for a report: the chat that asked, not the one that answered.
                    title={reported.length > 0 ? `Open ${nameOf(session)}` : undefined}
                  >
                    {reported.length > 0 ? (
                      <>
                        <MessageSquareReply className="node-icon" />
                        {`${reported.join(", ")} reported back`}
                      </>
                    ) : (
                      <>
                        <SquareTerminal className="node-icon" />
                        {nameOf(session)}
                      </>
                    )}
                  </button>
                </RovingFocusGroup.Item>
                <Ignore offer={ignore} onPress={onPress} />
              </li>
            );
          })}
        </ul>
      </RovingFocusGroup.Root>
      {unsaid}
    </div>
  );
}

/**
 * **A needs-you item's Ignore** (charter-app#248): the catalogue's `needs.ignore:<session>` row
 * drawn as the `✕` a pointer wants, so its accessible name is the row's words — "Ignore ide.3
 * until it asks again" — and the glyph is only the glyph.
 *
 * It drops the request and not the chat: the chat is still waiting, and its next stop puts it
 * back. The core holds that (`ignore_needs_you`), and the red counts on the project and
 * workspace tabs go down with the item because they are read from the same queue.
 *
 * **Not a Tab stop**, for the reason a tab's `×` is not (charter-app#189): the queue is one stop,
 * and fifty chats asking must not become a hundred. The keyboard's way to it is Delete on the
 * item ([`ignoreOnDelete`]) or the palette's row. Exported, with that handler, for whatever
 * lists the queue next — the title bar's list (charter-app#249).
 */
export function Ignore({ offer, onPress }: { offer?: Offer; onPress: (offer: Offer) => void }) {
  if (!offer) return null;
  return (
    <button
      className="needs-you-ignore"
      tabIndex={-1}
      aria-label={offer.title}
      title={offer.title}
      onClick={() => onPress(offer)}
    >
      <X />
    </button>
  );
}

/**
 * **Delete on a needs-you item ignores it**: the platform's delete key (`tabKeys.deletes`), as
 * it closes a focused tab (charter-app#239), pressing the same row the item's `✕` does.
 *
 * The keyboard moves to the next item — or the one before, for the last — as it goes. The item
 * leaves only when the core's answer arrives, and focus left on an element that then goes is
 * focus on the page, where the next Delete does nothing; so it moves now, while there is
 * somewhere to move it.
 */
export function ignoreOnDelete(
  event: KeyboardEvent<HTMLElement>,
  offer: Offer | undefined,
  onPress: (offer: Offer) => void,
) {
  if (offer === undefined || !deletes(event)) return;
  event.preventDefault();
  const item = event.currentTarget.closest("li");
  const neighbour = item?.nextElementSibling ?? item?.previousElementSibling;
  onPress(offer);
  neighbour?.querySelector<HTMLElement>("button")?.focus();
}
