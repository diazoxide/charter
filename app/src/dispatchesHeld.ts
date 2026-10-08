import { useCallback, useSyncExternalStore } from "react";
import { commands, type DispatchPending, type PlaneId } from "./bindings";
import { listen } from "./here";

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
    })
    // A chat whose held dispatches cannot be read shows none.
    .catch(() => {});
}

export function useDispatchesHeld(
  plane: PlaneId,
  session: number,
): { waiting: readonly DispatchPending[]; read: () => void } {
  const key = keyOf(plane, session);
  const subscribe = useCallback(
    (changed: () => void) => {
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
    },
    [key, plane, session],
  );
  const waiting = useSyncExternalStore(subscribe, () => held.get(key)?.waiting ?? NONE);
  const again = useCallback(() => read(plane, session), [plane, session]);
  return { waiting, read: again };
}
