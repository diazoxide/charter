import { useCallback, useEffect, useReducer, useRef, useSyncExternalStore } from "react";
import { commands, type PlaneId, type VaultRefused } from "./bindings";
import { listen } from "./here";

/**
 * **The vaults a chat was refused, held for the person** (#1456), read once per chat for
 * everything in the window that says something about them (#1486):
 *
 * - `VaultRefusedNotice` asks about the newest of them, on the pane of the tab the chat lives
 *   in, whichever chat that tab shows;
 * - a tab wears the hand for a chat of its own that was refused and is not on screen.
 *
 * Two readers of one answer, as `dispatchesHeld.ts` has it and for its reason: the hand cannot
 * stay on a tab after the Notice has been answered. The core holds what was refused; this reads
 * it when the first reader mounts and each time the core says a chat was refused
 * (`chat-vault-refused`), and `read` reads it again after a press. A list that cannot be read
 * shows nothing: the chat's own refusal still names the ways.
 */
type Held = {
  refused: readonly VaultRefused[];
  /** How many times the core has said this chat was refused, since the first reader: a Notice
   *  that had said what a press answered starts over when it grows. */
  heard: number;
  snapshot: { refused: readonly VaultRefused[]; heard: number };
  readers: Set<() => void>;
  stop?: () => void;
  gone: boolean;
};

const NONE: readonly VaultRefused[] = [];
const NOTHING = { refused: NONE, heard: 0 };
const held = new Map<string, Held>();
const keyOf = (plane: PlaneId, session: number) => `${plane}\u0000${session}`;

function tell(mine: Held) {
  mine.snapshot = { refused: mine.refused, heard: mine.heard };
  for (const changed of mine.readers) changed();
}

function read(plane: PlaneId, session: number) {
  void commands
    .vaultRefusals(plane, session)
    .then((answer) => {
      const mine = held.get(keyOf(plane, session));
      if (mine === undefined || answer.status !== "ok") return;
      // A core that answers nothing (an older one, a test's stand-in) holds none.
      mine.refused = Array.isArray(answer.data) ? answer.data : NONE;
      tell(mine);
    })
    .catch(() => {});
}

function subscribe(plane: PlaneId, session: number, changed: () => void): () => void {
  const key = keyOf(plane, session);
  let mine = held.get(key);
  if (mine === undefined) {
    const made: Held = {
      refused: NONE,
      heard: 0,
      snapshot: NOTHING,
      readers: new Set(),
      gone: false,
    };
    mine = made;
    held.set(key, made);
    read(plane, session);
    void (async () => {
      try {
        const unlisten = await listen<VaultRefused>("chat-vault-refused", (event) => {
          if (event.payload.plane !== plane || event.payload.session !== session) return;
          made.heard += 1;
          tell(made);
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

/** What chat `session` was refused, how many times the core has said so, and a way to read it
 *  again after a press. */
export function useVaultRefusals(
  plane: PlaneId,
  session: number,
): { refused: readonly VaultRefused[]; heard: number; read: () => void } {
  const sub = useCallback(
    (changed: () => void) => subscribe(plane, session, changed),
    [plane, session],
  );
  const { refused, heard } = useSyncExternalStore(
    sub,
    () => held.get(keyOf(plane, session))?.snapshot ?? NOTHING,
  );
  const again = useCallback(() => read(plane, session), [plane, session]);
  return { refused, heard, read: again };
}

/**
 * **Which of `sessions` have a refusal held for the person**, in the order given. One
 * subscription per chat, shared with that chat's Notice, so the two never disagree.
 */
export function useRefusedAmong(plane: PlaneId, sessions: readonly number[]): readonly number[] {
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
    (session) => (held.get(keyOf(plane, session))?.refused.length ?? 0) > 0,
  );
  return having.length === 0 ? NO_CHATS : having;
}

const NO_CHATS: readonly number[] = [];
