/**
 * What every chat is doing, kept current by being told rather than by asking.
 *
 * The core pushes a `chat-moved` event whenever a reader would see a difference (a hook
 * fired, or a program exited). Nothing here polls: at fifty live sessions a poll is fifty
 * questions a second to learn nothing, and the spec's whole point is that the answer arrives
 * from a hook.
 */
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { commands, type Moved, type OpenChat, type PlaneId } from "./bindings";

/** The five states the spec names. `unknown` is a harness that carries no state hook. */
export type State = "unknown" | "running" | "waiting" | "done" | "failed";

export type ChatStates = {
  /** By session. A session that is not here has never been heard from. */
  readonly bySession: Readonly<Record<number, State>>;
  /** Every chat asking for you, oldest first. This is the needs-you queue. */
  readonly needsYou: readonly number[];
  /**
   * When each chat last moved, as the core's own count — bigger is more recent.
   *
   * **The core's number, never one this window made up.** Charter ADR 0039 sorts the chat
   * strip's overflow menu by last activity and nothing in the window knows that fact: the
   * strip is an opening order and the queue is oldest-first. A count taken here would
   * restart at every launch and two windows on one plane would disagree about it, so it
   * comes down on `chat-moved` and in the first snapshot (`Moved.moved_at`).
   *
   * A session that is not here has never been heard from, and reads `0`.
   */
  readonly movedAt: Readonly<Record<number, number>>;
  /**
   * Which snapshot the queue came from (`Moved.sequence`), and which one each chat's state
   * came from (charter-app#248).
   *
   * **Snapshots reach the window in any order.** The core numbers each one under its board's
   * lock and sends it after letting go, on whichever thread built it, so a hook's report
   * taken just before a close can land just after it. Kept apart because they are different
   * facts: the queue is the whole board's, so only a newer snapshot replaces it; a chat's
   * state is only that chat's, so a snapshot that lost the race for the queue is still the
   * newest word about the chat it was about. `0` is "nothing yet"; the core numbers from 1.
   */
  readonly queueFrom: number;
  readonly heardAt: Readonly<Record<number, number>>;
};

export const nothingKnown: ChatStates = {
  bySession: {},
  needsYou: [],
  movedAt: {},
  queueFrom: 0,
  heardAt: {},
};

/** The state of one chat, which is `unknown` until something says otherwise. */
export function stateOf(states: ChatStates, session: number): State {
  return states.bySession[session] ?? "unknown";
}

/**
 * When a chat last moved, as the core counts moves on its plane. Bigger is more recent.
 *
 * `0` for a chat nothing has been heard about — which sorts last, and is honest: a window
 * that has been told nothing about a chat knows nothing about when it last did something.
 */
export function movedAt(states: ChatStates, session: number): number {
  return states.movedAt[session] ?? 0;
}

/**
 * The chats that can be waiting on the operator without saying so, by name.
 *
 * A chat whose harness cannot report everything — a Codex chat asking for approval mid-turn
 * says nothing — is named beside the queue, so an empty queue is never the app claiming
 * something it cannot see. Not one already in the queue, which is named there, and not one
 * whose program has ended, which cannot be waiting on anybody.
 */
export function quietOnes(
  chats: readonly OpenChat[],
  states: ChatStates,
  /** What the window calls a chat, when it has a name for it — a tab's (#270). */
  nameOf: (chat: OpenChat) => string = (chat) => chat.name,
): string[] {
  return chats
    .filter((chat) => Boolean(chat.unreported))
    .filter((chat) => !states.needsYou.includes(chat.session))
    .filter((chat) => !["done", "failed"].includes(stateOf(states, chat.session)))
    .map(nameOf);
}

/**
 * Applies one move, unless what it says is older than what is already known. Exported so a
 * test can drive the reducer without a window.
 *
 * **Older is strictly older.** A snapshot numbered the same as the one held is taken: the core
 * stops counting at the top of a `u32` rather than wrapping, and from there the window goes
 * back to taking snapshots in the order they land instead of refusing every one.
 */
export function moved(states: ChatStates, move: Moved): ChatStates {
  const newerQueue = move.sequence >= states.queueFrom;
  const newerChat = move.sequence >= (states.heardAt[move.session] ?? 0);
  if (!newerQueue && !newerChat) return states;
  return {
    bySession: newerChat
      ? { ...states.bySession, [move.session]: move.state as State }
      : states.bySession,
    // The whole queue travels on every move rather than being assembled here from a series
    // of edges: a window that missed one event would otherwise keep a chat in the queue, or
    // out of it, for as long as the app ran.
    needsYou: newerQueue ? move.queue : states.needsYou,
    // **Only the chat this event is about.** The count is per-chat and the event carries
    // one chat's, so folding it over the whole map would be writing this chat's number onto
    // every other chat — every tab would then read as having moved at once.
    movedAt: newerChat ? { ...states.movedAt, [move.session]: move.moved_at } : states.movedAt,
    queueFrom: newerQueue ? move.sequence : states.queueFrom,
    heardAt: newerChat ? { ...states.heardAt, [move.session]: move.sequence } : states.heardAt,
  };
}

/**
 * Subscribes to what the chats are doing in ONE plane, starting from what the core knows.
 *
 * The first answer matters: chats are put back before this plane's view subscribes (M1.7,
 * and charter-app#250's answer comes first), so some of them may have fired hooks already.
 *
 * **Every move is checked against the plane it came from.** `chat-moved` is emitted on the
 * app, not on a window, and every plane numbers its chats from one — so a process holding two
 * projects would otherwise have one project's "session 3 is waiting" land on the other's chat
 * 3. The event carries its plane precisely so this can be a comparison rather than a hope.
 */
export function useChatStates(plane: PlaneId | undefined): ChatStates {
  /**
   * What is known, and WHICH project it is known about.
   *
   * **The pair, because a state without its project is a guess.** Every project numbers its
   * chats from one, so a `waiting` left behind by the last project's chat 1 must not be drawn
   * on this one's — and `underneath` below deliberately never writes over what it finds, which
   * is right for a snapshot racing an event inside one project and exactly wrong across two.
   * Carried rather than cleared on the way in: a window that answered "what is chat 1 doing"
   * by forgetting, in an effect, would be answering it one render late.
   *
   * The opener is what made this reachable. Until a window could be given a second project,
   * there was no switch to be wrong about.
   */
  const [known, setKnown] = useState<{ plane?: PlaneId; states: ChatStates }>({
    states: nothingKnown,
  });
  /** Which plane's moves count, read at the moment one arrives. */
  const showing = useRef(plane);
  // Kept current in an effect rather than during the render, which is where a ref may be
  // written. Effects run in the order they are declared, so this lands before the listener
  // below is (re)registered and before any event this commit could deliver.
  useEffect(() => {
    showing.current = plane;
  }, [plane]);

  // **Listening starts at the mount and not when the plane is known**, and the two are
  // separate effects for that reason. The window learns its plane from a command, which
  // answers after the first paint — a listener that waited for it would be registered in the
  // window's second render and could be torn down mid-registration, and an event fired in
  // between would be lost with nothing to re-sync from.
  useEffect(() => {
    let gone = false;
    let stop: (() => void) | undefined;

    void (async () => {
      try {
        const unlisten = await listen<Moved>("chat-moved", (event) => {
          if (gone || event.payload.plane !== showing.current) return;
          setKnown((was) => ({
            plane: event.payload.plane,
            states: moved(
              was.plane === event.payload.plane ? was.states : nothingKnown,
              event.payload,
            ),
          }));
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in — a unit test, or a webview being torn down. The first
        // answer below is still worth having.
      }
    })();

    return () => {
      gone = true;
      stop?.();
    };
  }, []);

  // And the first answer, once there is a plane to ask about. Asked AFTER the listener is
  // registered above, which is what makes the fold below safe.
  useEffect(() => {
    if (plane === undefined) return;
    let gone = false;

    void (async () => {
      let known: Moved[];
      try {
        const answer = await commands.chatStates(plane);
        if (answer.status !== "ok") return;
        known = answer.data;
      } catch {
        return;
      }
      // **Underneath whatever has already arrived, never over it.** This used to fold the
      // snapshot on top, so an event that landed while the question was in flight was
      // overwritten by the older answer — a chat that had just gone to `waiting` dropped back
      // to `running` and out of the needs-you queue, and a chat waiting for you has no next
      // event to correct it. A review reproduced it.
      if (!gone && Array.isArray(known))
        setKnown((was) => ({
          plane,
          states: underneath(was.plane === plane ? was.states : nothingKnown, known),
        }));
    })();

    return () => {
      gone = true;
    };
  }, [plane]);

  // What is known about THIS project, and nothing at all about any other. Derived rather than
  // cleared: the answer is right in the render the project changes in, not one after it.
  return known.plane === plane ? known.states : nothingKnown;
}

/**
 * Folds a first snapshot under what has already arrived.
 *
 * **Under, because it is older**, and the numbers are what say so: the answer to
 * `chat_states` was read before any event that landed while it was in flight, so every part of
 * it that an event has since said something newer about is dropped by `moved` itself. This
 * used to be a guess from whether anything had been heard at all, which let a first answer's
 * queue stand over an event about a different chat.
 */
export function underneath(states: ChatStates, known: readonly Moved[]): ChatStates {
  return known.reduce(moved, states);
}
