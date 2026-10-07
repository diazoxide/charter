/**
 * What every chat is doing, kept current by being told rather than by asking.
 *
 * The core pushes a `chat-moved` event whenever a reader would see a difference (a hook
 * fired, or a program exited). Nothing here polls: at fifty live sessions a poll is fifty
 * questions a second to learn nothing, and the spec's whole point is that the answer arrives
 * from a hook.
 */
import { createContext, useContext, useEffect, useMemo, useRef, useState } from "react";
import { useSyncExternalStoreWithSelector } from "use-sync-external-store/with-selector";
import { listen } from "./here";

import {
  commands,
  type ChildAgent,
  type Moved,
  type Need,
  type OpenChat,
  type PlaneId,
} from "./bindings";

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
  /**
   * The chats that have reported back to each chat and not been read, by name (charter-app#259).
   * No needs-you item of its own (#1448): a chat that is in the queue for another reason says
   * `<child> reported back` there. Only the chat's own
   * snapshots change it, under `heardAt`'s rule, and its next prompt empties it.
   */
  readonly reports: Readonly<Record<number, readonly string[]>>;
  /**
   * What each chat's refused commits were refused for, one masked line each (SQ-16). A chat
   * with any is a needs-you item that says so. Under `heardAt`'s rule, like `reports`, and its
   * next prompt empties it.
   */
  readonly refusals: Readonly<Record<number, readonly string[]>>;
  /**
   * The chats each chat started that the operator stopped, by name (#1448): purlis's own word,
   * kept apart from `reports`, which are what chats said. Its row says `<child> was stopped`.
   * Under `heardAt`'s rule, like `reports`, and its next prompt empties it.
   */
  readonly stoppedBelow: Readonly<Record<number, readonly string[]>>;
  /**
   * Why each chat needs you when nothing it said itself says so, each as the sentence its
   * needs-you item says (#1448, `needSays`). Under `heardAt`'s rule, like `reports`, and its
   * next prompt empties it.
   */
  readonly needs: Readonly<Record<number, readonly string[]>>;
  /**
   * Each chat's child agents: the sub-agents and children its harness spawned, drawn under the
   * chat (FD-18, W8). Under `heardAt`'s rule, like `reports`: only the chat's own snapshots
   * change it.
   */
  readonly children: Readonly<Record<number, readonly ChildAgent[]>>;
};

export const nothingKnown: ChatStates = {
  bySession: {},
  needsYou: [],
  movedAt: {},
  queueFrom: 0,
  heardAt: {},
  reports: {},
  refusals: {},
  stoppedBelow: {},
  needs: {},
  children: {},
};

/** The state of one chat, which is `unknown` until something says otherwise. */
export function stateOf(states: ChatStates, session: number): State {
  return states.bySession[session] ?? "unknown";
}

/**
 * Whether a chat is a **shell tab** (SI-5): the operator's own shell, on no profile, running no
 * harness. The one answer the strip's mark, the explorer and the record all ask.
 */
export function isShell(chat: Pick<OpenChat, "harness" | "profile">): boolean {
  return chat.harness === null && chat.profile === null;
}

/**
 * The state mark a chat draws, or none.
 *
 * **A shell tab draws none until something reports a state for it.** Its terminal mark
 * already says what it is, and `unknown` beside it — a broken ring — read as a spinner on a
 * tab that is not waiting for anything. A harness started by hand in it whose hook report the
 * board adopts has said something, and the mark shows it as on any chat. A harness chat keeps
 * `unknown`: there it is the honest word for a harness that has not reported yet.
 */
export function markOf(states: ChatStates, session: number, shell: boolean): State | undefined {
  return states.bySession[session] ?? (shell ? undefined : "unknown");
}

/**
 * When a chat last moved, as the core counts moves across every plane. Bigger is more recent.
 *
 * `0` for a chat nothing has been heard about — which sorts last, and is honest: a window
 * that has been told nothing about a chat knows nothing about when it last did something.
 */
export function movedAt(states: ChatStates, session: number): number {
  return states.movedAt[session] ?? 0;
}

/** Whether `session` is in the needs-you queue. */
export function isAsking(states: ChatStates, session: number): boolean {
  return states.needsYou.includes(session);
}

/** The chats that reported back to `session` and have not been read yet, oldest first. */
export function reportsTo(states: ChatStates, session: number): readonly string[] {
  return states.reports[session] ?? [];
}

/** `session`'s child agents, oldest first: none for a chat that has spawned none. */
export function childrenOf(states: ChatStates, session: number): readonly ChildAgent[] {
  return states.children[session] ?? [];
}

/**
 * **What the chats a chat started have done since it last read**, as its item says it after
 * the chat's name: who reported back, and who was stopped (#1448). None when neither.
 */
export function backSaid(
  reported: readonly string[],
  stopped: readonly string[] = [],
): string | undefined {
  const said = [
    reported.length > 0 ? `${reported.join(", ")} reported back` : undefined,
    stopped.length === 1
      ? `${stopped[0]} was stopped`
      : stopped.length > 1
        ? `${stopped.join(", ")} were stopped`
        : undefined,
  ].filter((one) => one !== undefined);
  return said.length > 0 ? said.join("; ") : undefined;
}

/** Why `session` needs you beyond what it said itself, oldest first, as its item says each. */
export function needsOf(states: ChatStates, session: number): readonly string[] {
  return states.needs[session] ?? [];
}

/**
 * **What a needs-you item says of a need the app found** (#1448), after the chat's name and a
 * colon. One sentence per kind, written here once. A dispatch grant that is needed is the next
 * kind (#1437), and its sentence goes here.
 */
export function needSays(need: Need): string {
  switch (need.kind) {
    case "report_undelivered":
      // Closed, or still open with its program gone: either way nothing will read it.
      return `its report has nowhere to go because ${need.asker} has closed or its program has ended`;
  }
}

/** What `session`'s refused commits were refused for, oldest first. */
export function refusalsOf(states: ChatStates, session: number): readonly string[] {
  return states.refusals[session] ?? [];
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
    //
    // **Kept as the same array when it says the same thing** (SC-3): every reader of the queue
    // compares by identity, and a fresh copy of an unchanged queue on every move would redraw
    // all of them for a chat that only went from waiting to running.
    needsYou: newerQueue ? sameOr(states.needsYou, move.queue) : states.needsYou,
    // **Only the chat this event is about.** The count is per-chat and the event carries
    // one chat's, so folding it over the whole map would be writing this chat's number onto
    // every other chat — every tab would then read as having moved at once.
    movedAt: newerChat ? { ...states.movedAt, [move.session]: move.moved_at } : states.movedAt,
    queueFrom: newerQueue ? move.sequence : states.queueFrom,
    heardAt: newerChat ? { ...states.heardAt, [move.session]: move.sequence } : states.heardAt,
    reports: newerChat ? withLines(states.reports, move.session, move.reports) : states.reports,
    refusals: newerChat ? withLines(states.refusals, move.session, move.refusals) : states.refusals,
    stoppedBelow: newerChat
      ? withLines(states.stoppedBelow, move.session, move.stopped ?? undefined)
      : states.stoppedBelow,
    needs: newerChat
      ? withLines(states.needs, move.session, move.needs?.map(needSays))
      : states.needs,
    children: newerChat
      ? withChildren(states.children, move.session, move.children)
      : states.children,
  };
}

/**
 * `children` as `session`'s entry, and the same map when it says what the map already held, so
 * a chat whose children did not change keeps the list its rows were drawn from. A move with no
 * list — a core older than the field — says the chat has none.
 */
function withChildren(
  by: Readonly<Record<number, readonly ChildAgent[]>>,
  session: number,
  children: readonly ChildAgent[] | undefined,
): Readonly<Record<number, readonly ChildAgent[]>> {
  const now = Array.isArray(children) ? children : [];
  return sameChildren(by[session] ?? [], now) ? by : { ...by, [session]: now };
}

/** Whether two lists of child agents say the same thing: the same agents, in the same order and
 *  the same states. */
export function sameChildren(one: readonly ChildAgent[], other: readonly ChildAgent[]): boolean {
  return (
    one === other ||
    (one.length === other.length &&
      one.every((child, at) => child.agent === other[at].agent && child.state === other[at].state))
  );
}

/** Whether two lists hold the same things in the same order. */
export function sameList<T>(one: readonly T[], other: readonly T[]): boolean {
  return one === other || (one.length === other.length && one.every((it, at) => it === other[at]));
}

/** `was` when `now` says the same thing, so what did not change keeps its identity. A move
 *  that carries no list says it is empty, as `withLines` reads one. */
function sameOr<T>(was: readonly T[], now: readonly T[] | undefined): readonly T[] {
  const list = Array.isArray(now) ? now : [];
  return sameList(was, list) ? was : list;
}

/**
 * `lines` as `session`'s entry, and the same map when that is what it already held. A chat
 * nothing was said about has none, and an empty list says nothing new about it.
 *
 * A move that carries no list at all — a core older than the field — says the chat has none,
 * as it always has here: the move is still taken rather than dropped over it.
 */
function withLines(
  by: Readonly<Record<number, readonly string[]>>,
  session: number,
  lines: readonly string[] | undefined,
): Readonly<Record<number, readonly string[]>> {
  const now = Array.isArray(lines) ? lines : [];
  return sameList(by[session] ?? [], now) ? by : { ...by, [session]: now };
}

/**
 * What is known, and WHICH project it is known about.
 *
 * **The pair, because a state without its project is a guess.** Every project numbers its
 * chats from one, so a `waiting` left behind by the last project's chat 1 must not be drawn on
 * this one's — and `underneath` deliberately never writes over what it finds, which is right
 * for a snapshot racing an event inside one project and exactly wrong across two. Carried
 * rather than cleared on the way in: a window that answered "what is chat 1 doing" by
 * forgetting, in an effect, would be answering it one render late.
 *
 * The opener is what made this reachable. Until a window could be given a second project,
 * there was no switch to be wrong about.
 */
type Known = { readonly plane?: PlaneId; readonly states: ChatStates };

/**
 * **What every chat in a project is doing, held outside React** (SC-3, research 02 §5.6).
 *
 * It used to be a hook's state at the top of `PlaneView`, so every `chat-moved` re-rendered the
 * whole project view — every tab, every pane, and the catalogue — to change one dot. Held here,
 * each reader subscribes to its own share with `useSyncExternalStore` and is redrawn only when
 * that share changes: a tab to its own chat's state, the strips to the queue.
 */
export type ChatStore = {
  subscribe: (listener: () => void) => () => void;
  /** What is known about `plane`, and nothing at all about any other. */
  statesFor: (plane: PlaneId | undefined) => ChatStates;
};

/** A store and the project its readers are asking about: what `useChatsSelect` reads. */
export type Chats = { readonly store: ChatStore; readonly plane: PlaneId | undefined };

/** A store that is told things, which is the one `useChats` keeps. */
type HeldStore = ChatStore & { change: (how: (was: Known) => Known) => void };

function chatStore(): HeldStore {
  let known: Known = { states: nothingKnown };
  const listeners = new Set<() => void>();
  return {
    subscribe: (listener) => {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
    statesFor: (plane) => (known.plane === plane ? known.states : nothingKnown),
    change: (how) => {
      const now = how(known);
      if (now === known) return;
      known = now;
      for (const listener of [...listeners]) listener();
    },
  };
}

/**
 * Chats whose states are `states` and never move: what a component drawn on its own, in a test
 * or a preview, reads when nothing above it holds a project.
 */
export function fixedChats(states: ChatStates = nothingKnown): Chats {
  return { store: { subscribe: () => () => {}, statesFor: () => states }, plane: undefined };
}

/** The project's chats a row reads its own chat from. `PlaneView` provides it. */
export const ChatsHere = createContext<Chats>(fixedChats());

/** The chats of the project this component is drawn in. */
export function useChatsHere(): Chats {
  return useContext(ChatsHere);
}

/**
 * **One reader's share of what the chats are doing**, and a re-render only when it changes.
 *
 * `select` picks the share and `same` says whether two of them are the same answer, so a share
 * the reducer rebuilt with the same contents — a list of names, an order — does not redraw.
 * React's own selector hook (`use-sync-external-store/with-selector`) does the remembering, so
 * the rule `useSyncExternalStore` has — the same snapshot answers the same value — is kept by
 * the standard implementation rather than by one written here.
 */
export function useChatsSelect<T>(
  chats: Chats,
  select: (states: ChatStates) => T,
  same: (one: T, other: T) => boolean = Object.is,
): T {
  const { store, plane } = chats;
  const states = () => store.statesFor(plane);
  return useSyncExternalStoreWithSelector(store.subscribe, states, states, select, same);
}

/**
 * Keeps what the chats are doing in ONE plane, starting from what the core knows.
 *
 * The first answer matters: chats are put back before this plane's view subscribes (M1.7,
 * and charter-app#250's answer comes first), so some of them may have fired hooks already.
 *
 * **Every move is checked against the plane it came from.** `chat-moved` is emitted on the
 * app, not on a window, and every plane numbers its chats from one — so a process holding two
 * projects would otherwise have one project's "session 3 is waiting" land on the other's chat
 * 3. The event carries its plane precisely so this can be a comparison rather than a hope.
 *
 * **It answers the store, not the states**, so the component holding it is not redrawn by a
 * move: only what reads a share through `useChatsSelect` is.
 */
export function useChats(plane: PlaneId | undefined): Chats {
  const [store] = useState(chatStore);
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
          store.change((was) => {
            const states = moved(
              was.plane === event.payload.plane ? was.states : nothingKnown,
              event.payload,
            );
            return states === was.states && was.plane === event.payload.plane
              ? was
              : { plane: event.payload.plane, states };
          });
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
  }, [store]);

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
        store.change((was) => ({
          plane,
          states: underneath(was.plane === plane ? was.states : nothingKnown, known),
        }));
    })();

    return () => {
      gone = true;
    };
  }, [plane, store]);

  // What is known about THIS project, and nothing at all about any other: `statesFor` derives
  // it rather than clearing it, so the answer is right in the render the project changes in.
  return useMemo(() => ({ store, plane }), [store, plane]);
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
