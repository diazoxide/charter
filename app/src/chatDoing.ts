/**
 * What each working chat is doing, in one line (#1493, V100-42), kept current by being told.
 *
 * The core sorts what a chat's tool hooks say into a fixed list of kinds, with at most one
 * short name it has passed (a file's base name, or a program from its fixed list), and whether
 * the tool has come back, and sends `chat-doing`
 * each time a chat's line changes, at most a few times a second per chat. This holds the
 * latest per chat, outside React, as the chats' states are held (`chatState.ts`, SC-3): a
 * reader subscribes to its own chat's line and is redrawn only when that line changes.
 *
 * **In memory only.** Nothing here is saved, and the core wrote it nowhere either.
 *
 * **The sentences are this file's**, one per kind ({@link doingSays}), and a kind it does not
 * know says nothing at all. So no line is ever anything but one of these sentences with, at
 * most, the one name in it.
 */
import { createContext, useContext, useEffect, useMemo, useRef, useState } from "react";
import { useSyncExternalStoreWithSelector } from "use-sync-external-store/with-selector";
import { listen } from "./here";
import { markOf, useChatsHere, useChatsSelect } from "./chatState";
import { commands, type ChatDoing, type Doing, type PlaneId } from "./bindings";

/** What is known of what the chats are doing. */
export type Doings = {
  /** By session. A session that is not here has no line. */
  readonly bySession: Readonly<Record<number, Doing>>;
  /** Which telling each chat's line came from (`ChatDoing.sequence`): they land in any order. */
  readonly heardAt: Readonly<Record<number, number>>;
};

const NOTHING: Doings = { bySession: {}, heardAt: {} };

/** Whether two lines say the same thing. */
export function sameDoing(one: Doing | undefined, other: Doing | undefined): boolean {
  return (
    one === other ||
    (one !== undefined &&
      other !== undefined &&
      one.kind === other.kind &&
      one.name === other.name &&
      one.count === other.count &&
      one.over === other.over)
  );
}

/**
 * Applies one telling, unless it is older than what is held of its chat. Older is strictly
 * older, as a chat's move is read (`chatState.moved`).
 */
export function told(doings: Doings, one: ChatDoing): Doings {
  if (one.sequence < (doings.heardAt[one.session] ?? 0)) return doings;
  const heardAt = { ...doings.heardAt, [one.session]: one.sequence };
  const was = doings.bySession[one.session];
  const now = one.doing ?? undefined;
  if (sameDoing(was, now)) return { bySession: doings.bySession, heardAt };
  if (now !== undefined) return { bySession: { ...doings.bySession, [one.session]: now }, heardAt };
  const bySession = Object.fromEntries(
    Object.entries(doings.bySession).filter(([session]) => Number(session) !== one.session),
  );
  return { bySession, heardAt };
}

/** What a line says: purlis's own words, and the one name where the kind has one. */
export type Says = {
  /** The words, with the name left out of them. */
  readonly words: string;
  /** The name that follows the words, shown as text and kept in its own direction. */
  readonly name?: string;
};

/**
 * The sentences, all of them: for each kind, what is said while its tool is in flight and what
 * is said once a hook has said the tool came back. A kind that is not here says nothing.
 */
const SENTENCES: ReadonlyMap<string, { readonly now: string; readonly over?: string }> = new Map([
  // Said only from a turn's start until the first tool heard in it: it has no past.
  ["thinking", { now: "thinking" }],
  ["command", { now: "running a command", over: "ran a command" }],
  ["editing", { now: "editing a file", over: "edited a file" }],
  ["reading", { now: "reading a file", over: "read a file" }],
  ["searching", { now: "searching", over: "searched" }],
  ["fetching", { now: "fetching a page", over: "fetched a page" }],
  ["helper", { now: "waiting on a helper", over: "a helper finished" }],
  ["dispatching", { now: "dispatching a task", over: "dispatched a task" }],
  ["asking", { now: "asking a question", over: "asked a question" }],
  ["reporting", { now: "writing its report", over: "wrote its report" }],
  ["tool", { now: "using a tool", over: "used a tool" }],
]);

/** The words in front of a name, for the kinds that have one: in flight, and once back. */
const NAMED: ReadonlyMap<string, { readonly now: string; readonly over: string }> = new Map([
  ["command", { now: "running", over: "ran" }],
  ["editing", { now: "editing", over: "edited" }],
  ["reading", { now: "reading", over: "read" }],
]);

/**
 * What a name may be made of, held here as well as in the core, which already passed it
 * (`purlis_core::doing`): ASCII letters and digits and seven marks, at most 48 characters. So
 * the window by itself never shows a name with a space, an invisible character, a letter of
 * another script or markup in it, whatever it is sent.
 */
const A_NAME = /^[A-Za-z0-9._+@~#-]{1,48}$/;

/**
 * What `doing` says, or nothing for a kind this window has no sentence for (a newer core's):
 * it never shows a word it was sent. In the present while the tool is in flight, in the past
 * once it has come back (`over`).
 */
export function doingSays(doing: Doing): Says | undefined {
  const plain = SENTENCES.get(doing.kind);
  if (plain === undefined) return undefined;
  const over = doing.over === true && plain.over !== undefined;
  if (doing.kind === "reading" && doing.count > 1)
    return { words: `${over ? "read" : "reading"} ${doing.count} files` };
  const before = NAMED.get(doing.kind);
  if (before !== undefined && typeof doing.name === "string" && A_NAME.test(doing.name))
    return { words: over ? before.over : before.now, name: doing.name };
  return { words: over && plain.over !== undefined ? plain.over : plain.now };
}

/** `says` as one string: the line's tooltip, and what a screen reader is told on demand. */
export function saidWhole(says: Says): string {
  return says.name === undefined ? says.words : `${says.words} ${says.name}`;
}

type Known = { readonly plane?: PlaneId; readonly doings: Doings };

/** What a project's chats are doing, held outside React. */
export type DoingStore = {
  subscribe: (listener: () => void) => () => void;
  /** What is known about `plane`, and nothing at all about any other. */
  doingsFor: (plane: PlaneId | undefined) => Doings;
};

/** A store and the project its readers ask about. */
export type DoingsOf = { readonly store: DoingStore; readonly plane: PlaneId | undefined };

type HeldStore = DoingStore & { change: (how: (was: Known) => Known) => void };

function doingStore(): HeldStore {
  let known: Known = { doings: NOTHING };
  const listeners = new Set<() => void>();
  return {
    subscribe: (listener) => {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
    doingsFor: (plane) => (known.plane === plane ? known.doings : NOTHING),
    change: (how) => {
      const now = how(known);
      if (now === known) return;
      known = now;
      for (const listener of [...listeners]) listener();
    },
  };
}

/** Lines that are `doings` and never change: for a component drawn on its own. */
export function fixedDoings(doings: Doings = NOTHING): DoingsOf {
  return { store: { subscribe: () => () => {}, doingsFor: () => doings }, plane: undefined };
}

/** What the chats of the project a row is drawn in are doing. `PlaneView` provides it. */
export const DoingsHere = createContext<DoingsOf>(fixedDoings());

/**
 * What chat `session` is doing, and a redraw only when that changes: another chat's line, and
 * a telling that says the same thing again, redraw nothing here.
 */
export function useDoingOf(session: number): Doing | undefined {
  const { store, plane } = useContext(DoingsHere);
  const doings = () => store.doingsFor(plane);
  return useSyncExternalStoreWithSelector(
    store.subscribe,
    doings,
    doings,
    (all) => all.bySession[session],
    sameDoing,
  );
}

/**
 * What chat `session` is doing now, as its row says it, or nothing: its turn is not running,
 * nothing was heard from its hooks, or the core said a kind this window has no sentence for.
 *
 * **Only while the board has the chat running**, whatever line is held of it: a chat that
 * ended, or one waiting on the person, wears no stale one.
 */
export function useDoingSaid(session: number): Says | undefined {
  const running = useChatsSelect(
    useChatsHere(),
    (states) => markOf(states, session, false) === "running",
  );
  const doing = useDoingOf(session);
  return running && doing !== undefined ? doingSays(doing) : undefined;
}

/**
 * Keeps what the chats of ONE project are doing, starting from what the core holds. It
 * answers the store and not the lines, so the component holding it is not redrawn by one.
 *
 * As `useChats`: listening starts at the mount, every telling is checked against the project
 * it came from, and the first answer is folded under whatever has arrived meanwhile.
 */
export function useDoings(plane: PlaneId | undefined): DoingsOf {
  const [store] = useState(doingStore);
  const showing = useRef(plane);
  useEffect(() => {
    showing.current = plane;
  }, [plane]);

  useEffect(() => {
    let gone = false;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unlisten = await listen<ChatDoing>("chat-doing", (event) => {
          const one = event.payload;
          if (gone || one.plane !== showing.current) return;
          store.change((was) => {
            const from = was.plane === one.plane ? was.doings : NOTHING;
            return { plane: one.plane, doings: told(from, one) };
          });
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in: a unit test, or a webview being torn down.
      }
    })();
    return () => {
      gone = true;
      stop?.();
    };
  }, [store]);

  useEffect(() => {
    if (plane === undefined) return;
    let gone = false;
    void (async () => {
      let known: ChatDoing[];
      try {
        const answer = await commands.chatDoings(plane);
        if (answer.status !== "ok") return;
        known = answer.data;
      } catch {
        return;
      }
      if (!gone && Array.isArray(known))
        store.change((was) => ({
          plane,
          doings: known.reduce(told, was.plane === plane ? was.doings : NOTHING),
        }));
    })();
    return () => {
      gone = true;
    };
  }, [plane, store]);

  return useMemo(() => ({ store, plane }), [store, plane]);
}
