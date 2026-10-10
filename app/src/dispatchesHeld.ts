import { useCallback, useEffect, useReducer, useRef, useSyncExternalStore } from "react";
import { commands, type DispatchPending, type PlaneId } from "./bindings";
import { listen } from "./here";
import { asksMoved } from "./asks";

/**
 * **The dispatches a chat has asked for that are held for the person** (#1437), read once per
 * chat for everything on its pane that says something about them (#1481):
 *
 * - `DispatchGrantNotice` asks about the first of them;
 * - `VaultRefusedNotice` points to that question, where the chat has already asked the persona
 *   the refused vault is tagged for, and offers no second way to ask it.
 *
 * Two readers of one answer, so neither can say a dispatch is waiting after the other has seen
 * it answered: the core holds them, this reads them when the first reader mounts and each time
 * the core says one is needed (`dispatch-grant-needed`), and `read` reads them again after a
 * press. A list that cannot be read is no list: nothing starts unasked.
 */
type Held = {
  waiting: readonly DispatchPending[];
  readers: Set<() => void>;
  stop?: () => void;
  gone: boolean;
};

const NONE: readonly DispatchPending[] = [];
const held = new Map<string, Held>();
const keyOf = (plane: PlaneId, session: number) => `${plane}\u0000${session}`;

function read(plane: PlaneId, session: number) {
  void commands
    .dispatchGrantsNeeded(plane, session)
    .then((answer) => {
      const mine = held.get(keyOf(plane, session));
      if (mine === undefined || answer.status !== "ok") return;
      // A core that answers nothing holds nothing.
      mine.waiting = answer.data ?? NONE;
      for (const changed of mine.readers) changed();
      // The Inbox lists the same dispatches from the asks registry (#1692): an answer on the
      // pane's Notice clears its copy there too.
      asksMoved(plane);
    })
    // A chat whose held dispatches cannot be read shows none.
    .catch(() => {});
}

function subscribe(plane: PlaneId, session: number, changed: () => void): () => void {
  const key = keyOf(plane, session);
  let mine = held.get(key);
  if (mine === undefined) {
    const made: Held = { waiting: NONE, readers: new Set(), gone: false };
    mine = made;
    held.set(key, made);
    read(plane, session);
    void (async () => {
      try {
        const unlisten = await listen<DispatchPending>("dispatch-grant-needed", (event) => {
          if (event.payload.plane !== plane || event.payload.session !== session) return;
          read(plane, session);
        });
        if (made.gone) unlisten();
        else made.stop = unlisten;
      } catch {
        // No window to listen in: a unit test, or a webview being torn down.
      }
    })();
  }
  const reading = mine;
  reading.readers.add(changed);
  return () => {
    reading.readers.delete(changed);
    if (reading.readers.size > 0) return;
    reading.gone = true;
    reading.stop?.();
    if (held.get(key) === reading) held.delete(key);
  };
}

export function useDispatchesHeld(
  plane: PlaneId,
  session: number,
): { waiting: readonly DispatchPending[]; read: () => void } {
  const key = keyOf(plane, session);
  const sub = useCallback(
    (changed: () => void) => subscribe(plane, session, changed),
    [plane, session],
  );
  const waiting = useSyncExternalStore(sub, () => held.get(key)?.waiting ?? NONE);
  const again = useCallback(() => read(plane, session), [plane, session]);
  return { waiting, read: again };
}

/**
 * **Which of `sessions` have a dispatch held for the person** (#1486), in the order given:
 * what a tab wears the hand for when the chat is one of its own and is not on screen. One
 * subscription per chat, shared with that chat's Notice, so the hand goes when the Notice is
 * answered.
 */
export function useHeldAmong(plane: PlaneId, sessions: readonly number[]): readonly number[] {
  const wanted = sessions.join(",");
  /** Each chat's subscription, kept while the chat stays in the list: a chat joining or
   *  leaving the list reads that chat and no other. */
  const reading = useRef(new Map<number, () => void>());
  const [, changed] = useReducer((count: number) => count + 1, 0);
  // Every subscription goes with the project. Declared first, so it has let go before the
  // effect below subscribes for the next one.
  useEffect(() => {
    const mine = reading.current;
    return () => {
      for (const stop of mine.values()) stop();
      mine.clear();
    };
  }, [plane]);
  useEffect(() => {
    const want = new Set(wanted === "" ? [] : wanted.split(",").map(Number));
    for (const [session, stop] of reading.current) {
      if (want.has(session)) continue;
      stop();
      reading.current.delete(session);
    }
    for (const session of want)
      if (!reading.current.has(session))
        reading.current.set(session, subscribe(plane, session, changed));
  }, [plane, wanted]);
  const having = sessions.filter(
    (session) => (held.get(keyOf(plane, session))?.waiting.length ?? 0) > 0,
  );
  return having.length === 0 ? NO_CHATS : having;
}

const NO_CHATS: readonly number[] = [];
