import { useSyncExternalStore } from "react";
import type { ChatStates } from "./chatState";
import { shownOfRow } from "./chatsList";
import type { ChatRow } from "./chatsTree";
import type { ShownKind } from "./shownState";

/**
 * **How long each chat has been in its state, as this window saw it** (#1499, V100-19).
 *
 * The core sends a count of moves and no clock (`Moved.moved_at`), so the time is this
 * window's own: it is taken when the window sees a chat's shown state change. **Nothing is
 * guessed.** A chat that was already in its state when the window first read it has no time,
 * and its row says none, and the same holds for the first thing the window hears of a chat it
 * had heard nothing from: that may be the board's first answer about a state that is hours old.
 * A chat that arrived while the window was open is timed from its arrival.
 */
export type StateClock = {
  /** Reads every row's state as it now stands, at `now` (ms), noting what changed. */
  read: (states: ChatStates, rows: readonly ChatRow[], now: number) => void;
  /** When chat `session` came into its state (ms), or nothing where the window did not see. */
  since: (session: number) => number | null;
  subscribe: (listener: () => void) => () => void;
};

type Seen = {
  kind: ShownKind | undefined;
  at: number | null;
  /** Whether the window had heard of it before its state last changed. */
  heard: boolean;
};

export function stateClock(): StateClock {
  const seen = new Map<number, Seen>();
  const listeners = new Set<() => void>();
  /** Whether a list of chats has been read: a chat first seen after that has just arrived. */
  let read = false;
  return {
    read: (states, rows, now) => {
      const queue = new Set(states.needsYou);
      let changed = false;
      for (const row of rows) {
        const kind = shownOfRow(states, row, queue)?.kind;
        const heard = (states.movedAt[row.session] ?? 0) > 0;
        const was = seen.get(row.session);
        if (was === undefined) {
          seen.set(row.session, { kind, at: read ? now : null, heard: read || heard });
          changed ||= read;
          continue;
        }
        if (was.kind !== kind) {
          const at = was.heard ? now : null;
          changed ||= at !== was.at;
          was.kind = kind;
          was.at = at;
        }
        was.heard ||= heard;
      }
      if (rows.length > 0) read = true;
      if (seen.size > rows.length) {
        const listed = new Set(rows.map((row) => row.session));
        for (const session of [...seen.keys()]) if (!listed.has(session)) seen.delete(session);
      }
      if (changed) for (const listener of [...listeners]) listener();
    },
    since: (session) => seen.get(session)?.at ?? null,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
  };
}

/** How often a row's time is read again. A row says minutes, so half of one is enough. */
const TICK_MS = 30_000;
let nowMs = 0;
let ticking: ReturnType<typeof setInterval> | undefined;
const tickers = new Set<() => void>();

function everyTick(listener: () => void): () => void {
  tickers.add(listener);
  if (ticking === undefined) {
    nowMs = Date.now();
    ticking = setInterval(() => {
      nowMs = Date.now();
      for (const one of [...tickers]) one();
    }, TICK_MS);
  }
  return () => {
    tickers.delete(listener);
    if (tickers.size === 0) {
      clearInterval(ticking);
      ticking = undefined;
    }
  };
}

/**
 * How many seconds chat `session` has been in its state, or nothing where the window did not
 * see it change. One clock for every row, read twice a minute, and a row is drawn again by it
 * only through the component that calls this.
 */
export function useStateSince(clock: StateClock, session: number): number | null {
  const at = useSyncExternalStore(clock.subscribe, () => clock.since(session));
  const now = useSyncExternalStore(everyTick, () => nowMs);
  return at === null ? null : Math.max(0, Math.floor((now - at) / 1000));
}
