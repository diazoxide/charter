import { useSyncExternalStore } from "react";
import type { ChatStates } from "./chatState";
import { standingOfRows } from "./sessionTasks";
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
 * A chat that arrived while the window was open is timed from its arrival, and it is known to
 * have arrived only by its number being higher than any the clock has read: the list read again
 * after being empty or partial brings back old chats, which are not arrivals.
 */
export type StateClock = {
  /**
   * Reads every row's state as it now stands, at `now` (ms), noting what changed. `drawn` is
   * the chats whose rows are on screen: a change to any other is one its row did not show.
   */
  read: (
    states: ChatStates,
    rows: readonly ChatRow[],
    now: number,
    drawn?: ReadonlySet<number>,
  ) => void;
  /**
   * Whether chat `session`'s state changed while its row was not drawn, asked once as its row
   * is drawn again: a task that started needing the person under a folded session arrives in
   * that state, though its row never showed another.
   */
  missed: (session: number) => boolean;
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
  /** The chats whose state changed while their rows were not drawn. */
  const unseen = new Set<number>();
  /** The highest chat number read so far, once a list has been: a chat first seen with a
   *  higher one has just arrived. Chats are numbered in the order they start. */
  let highest: number | undefined;
  return {
    read: (states, rows, now, drawn) => {
      // Every row's state from the one pass, so the clock times the word the row draws.
      const stood = standingOfRows(states, rows);
      let changed = false;
      for (const row of rows) {
        const kind = stood.get(row.session)?.shown?.kind;
        const heard = (states.movedAt[row.session] ?? 0) > 0;
        const was = seen.get(row.session);
        if (was === undefined) {
          const arrived = highest !== undefined && row.session > highest;
          seen.set(row.session, { kind, at: arrived ? now : null, heard: arrived || heard });
          changed ||= arrived;
          continue;
        }
        if (was.kind !== kind) {
          if (drawn !== undefined && !drawn.has(row.session)) unseen.add(row.session);
          else unseen.delete(row.session);
          const at = was.heard ? now : null;
          changed ||= at !== was.at;
          was.kind = kind;
          was.at = at;
        }
        was.heard ||= heard;
      }
      for (const row of rows) highest = Math.max(highest ?? 0, row.session);
      if (seen.size > rows.length) {
        const listed = new Set(rows.map((row) => row.session));
        for (const session of [...seen.keys()]) {
          if (listed.has(session)) continue;
          seen.delete(session);
          unseen.delete(session);
        }
      }
      if (changed) for (const listener of [...listeners]) listener();
    },
    since: (session) => seen.get(session)?.at ?? null,
    missed: (session) => unseen.delete(session),
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
  const now = useSyncExternalStore(
    everyTick,
    () => nowMs,
    () => nowMs,
  );
  return at === null ? null : Math.max(0, Math.floor((now - at) / 1000));
}
