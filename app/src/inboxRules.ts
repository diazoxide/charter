/**
 * **The Inbox's own rules** (#1692, spec #1688, I-6, I-12): how a project's asks are grouped and
 * ordered, what was answered here lately, and the bytes a reply is typed as. No commands and no
 * markup, so each rule is tested on its own; `Inbox.tsx` draws them.
 */
import { useSyncExternalStore } from "react";
import type { Shown } from "./bindings";

/** The asks of one chat, as the Inbox draws them under its chain. */
export type ChatGroup = {
  session: number;
  /** Who is asking, the session first and the chat that asked last (I-9): names, as data. */
  chain: readonly string[];
  asks: readonly Shown[];
};

/** The key an ask is known by across reads of its project's list. */
export const askKey = (ask: Shown) => `${ask.session}\u0000${ask.ask}`;

/**
 * **When this window first saw each ask**, by project, as a sequence and not a clock: the
 * registry derives each list afresh and carries no time of its own (I-8), so "oldest" is the
 * ask that has been on the list longest. Kept for the window's life, so a project put behind
 * another and brought back keeps its order.
 */
const seen = new Map<string, Map<string, number>>();
let seq = 0;

/** Notes the asks of `plane` this window has not seen before, and forgets the ones gone. */
export function noteSeen(plane: string, asks: readonly Shown[]): void {
  const mine = seen.get(plane) ?? new Map<string, number>();
  seen.set(plane, mine);
  const now = new Set(asks.map(askKey));
  for (const key of mine.keys()) if (!now.has(key)) mine.delete(key);
  for (const ask of asks) if (!mine.has(askKey(ask))) mine.set(askKey(ask), ++seq);
}

/** Where `ask` stands in the order `plane`'s asks were first seen in: the lower, the older. */
export function seenOrder(plane: string, ask: Shown): number {
  return seen.get(plane)?.get(askKey(ask)) ?? Number.MAX_SAFE_INTEGER;
}

/**
 * **The asks of one project, grouped by chat, oldest first** (I-6): each chat's group stands
 * where its oldest ask does, and inside a group the asks stand oldest first. Asks seen in the
 * same read keep the registry's order, which is its sources' order.
 */
export function byChat(asks: readonly Shown[], age: (ask: Shown) => number): ChatGroup[] {
  const ordered = asks
    .map((ask, at) => ({ ask, at, age: age(ask) }))
    .sort((one, other) => one.age - other.age || one.at - other.at)
    .map(({ ask }) => ask);
  const groups = new Map<number, Shown[]>();
  for (const ask of ordered) {
    const group = groups.get(ask.session) ?? [];
    group.push(ask);
    groups.set(ask.session, group);
  }
  return [...groups.entries()].map(([session, mine]) => ({
    session,
    chain: mine[0].chain.length > 0 ? mine[0].chain : [`chat ${session}`],
    asks: mine,
  }));
}

/** What the Inbox keeps of an ask once it was answered from it (I-12): read-only. */
export type Answered = {
  /** When, in ms since the epoch. */
  at: number;
  chain: readonly string[];
  says: string;
  /** The answer as its button read, or a reply's words. */
  answer: string;
};

/** The most answered asks a project's Inbox lists. */
export const MOST_ANSWERED = 10;

/**
 * **What was answered from the Inbox, by project, newest first** (I-12). In memory: it is a
 * look back over what the person agreed to while the window was open, never a record, and the
 * sources keep their own (the network record, the grants).
 */
const answered = new Map<string, readonly Answered[]>();
const readers = new Set<() => void>();
const NONE: readonly Answered[] = [];

export function noteAnswered(plane: string, one: Answered): void {
  answered.set(plane, [one, ...(answered.get(plane) ?? NONE)].slice(0, MOST_ANSWERED));
  for (const changed of readers) changed();
}

/** What `plane`'s Inbox answered lately, newest first. */
export function useAnswered(plane: string): readonly Answered[] {
  return useSyncExternalStore(
    (changed) => {
      readers.add(changed);
      return () => readers.delete(changed);
    },
    () => answered.get(plane) ?? NONE,
  );
}

/** Forgets every project's order and answers: for a test, which starts with none. */
export function forgetInbox(): void {
  seen.clear();
  answered.clear();
  seq = 0;
  for (const changed of readers) changed();
}

/** Whether `c` is a C0 or C1 control character, or DEL. */
const isControl = (c: string) => {
  const code = c.codePointAt(0) ?? 0;
  return code < 0x20 || (code >= 0x7f && code <= 0x9f);
};

/** What starts and ends a bracketed paste. */
const PASTE_BEGINS = "\u001b[200~";
const PASTE_ENDS = "\u001b[201~";

/**
 * **A reply, as the bytes a chat is sent** (#1692): one bracketed paste, then Enter, in one
 * write, as purlis types any line into a chat (`curation::bracketed`), so a harness reads it as
 * the person's message and a line break inside it does not send half of it. Every control
 * character but a line break is dropped first, so nothing typed can end the paste early or reach
 * the terminal as a key. Nothing at all for a reply with nothing in it.
 */
export function replyBytes(text: string): string | undefined {
  const plain = [...text.replace(/\r\n?/g, "\n")]
    .filter((c) => c === "\n" || !isControl(c))
    .join("");
  if (plain.trim() === "") return undefined;
  return `${PASTE_BEGINS}${plain}${PASTE_ENDS}\r`;
}
