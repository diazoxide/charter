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
};

export const nothingKnown: ChatStates = { bySession: {}, needsYou: [] };

/** The state of one chat, which is `unknown` until something says otherwise. */
export function stateOf(states: ChatStates, session: number): State {
  return states.bySession[session] ?? "unknown";
}

/**
 * The chats that can be waiting on the operator without saying so, by name.
 *
 * A chat whose harness cannot report everything — a Codex chat asking for approval mid-turn
 * says nothing — is named beside the queue, so an empty queue is never the app claiming
 * something it cannot see. Not one already in the queue, which is named there, and not one
 * whose program has ended, which cannot be waiting on anybody.
 */
export function quietOnes(chats: readonly OpenChat[], states: ChatStates): string[] {
  return chats
    .filter((chat) => Boolean(chat.unreported))
    .filter((chat) => !states.needsYou.includes(chat.session))
    .filter((chat) => !["done", "failed"].includes(stateOf(states, chat.session)))
    .map((chat) => chat.name);
}

/** Applies one move. Exported so a test can drive the reducer without a window. */
export function moved(states: ChatStates, move: Moved): ChatStates {
  return {
    bySession: { ...states.bySession, [move.session]: move.state as State },
    // The whole queue travels on every move rather than being assembled here from a series
    // of edges: a window that missed one event would otherwise keep a chat in the queue, or
    // out of it, for as long as the app ran.
    needsYou: move.queue,
  };
}

/**
 * Subscribes to what the chats are doing in ONE plane, starting from what the core knows.
 *
 * The first answer matters: chats are put back before there is a window (M1.7), so some of
 * them may have fired hooks already.
 *
 * **Every move is checked against the plane it came from.** `chat-moved` is emitted on the
 * app, not on a window, and every plane numbers its chats from one — so a process holding two
 * projects would otherwise have one project's "session 3 is waiting" land on the other's chat
 * 3. The event carries its plane precisely so this can be a comparison rather than a hope.
 */
export function useChatStates(plane: PlaneId | undefined): ChatStates {
  const [states, setStates] = useState<ChatStates>(nothingKnown);
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
          if (gone) return;
          setStates((states) => moved(states, event.payload));
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
      if (!gone && Array.isArray(known)) setStates((states) => underneath(states, known));
    })();

    return () => {
      gone = true;
    };
  }, [plane]);

  return states;
}

/**
 * Folds a first snapshot under what has already arrived.
 *
 * A session an event has already touched keeps what the event said; the queue is the
 * snapshot's only if no event has landed at all, because an event's queue is newer than any
 * answer to a question asked before it.
 */
export function underneath(states: ChatStates, known: readonly Moved[]): ChatStates {
  const heard = Object.keys(states.bySession).length > 0;
  const bySession = { ...states.bySession };
  for (const one of known) {
    if (!(one.session in bySession)) bySession[one.session] = one.state as State;
  }
  return {
    bySession,
    needsYou: heard ? states.needsYou : (known[known.length - 1]?.queue ?? []),
  };
}
