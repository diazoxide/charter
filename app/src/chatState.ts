/**
 * What every chat is doing, kept current by being told rather than by asking.
 *
 * The core pushes a `chat-moved` event whenever a reader would see a difference (a hook
 * fired, or a program exited). Nothing here polls: at fifty live sessions a poll is fifty
 * questions a second to learn nothing, and the spec's whole point is that the answer arrives
 * from a hook.
 */
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { commands, type Moved } from "./bindings";

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
 * Subscribes to what the chats are doing, starting from what the core already knows.
 *
 * The first answer matters: chats are put back before there is a window (M1.7), so some of
 * them may have fired hooks already.
 */
export function useChatStates(): ChatStates {
  const [states, setStates] = useState<ChatStates>(nothingKnown);

  useEffect(() => {
    let gone = false;
    let stop: (() => void) | undefined;

    void (async () => {
      // Listening BEFORE asking, and awaited: `listen` registers asynchronously, so an event
      // emitted between the answer arriving and the listener being ready would otherwise be
      // lost with nothing to re-sync from.
      try {
        const unlisten = await listen<Moved>("chat-moved", (event) => {
          if (!gone) setStates((states) => moved(states, event.payload));
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in — a unit test, or a webview being torn down. The first
        // answer below is still worth having.
      }

      let known: Moved[];
      try {
        known = await commands.chatStates();
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
      stop?.();
    };
  }, []);

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
