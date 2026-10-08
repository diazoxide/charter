import { useSyncExternalStore } from "react";
import { atCreation, sayAboutThisMachine, type Reading } from "./windowprefs";

/**
 * **How the Chats list is drawn** (#1499, V100-73): whether a row is two lines or one, and
 * whether the sessions of one workspace stand together.
 *
 * **Kept in the layout file, beside the text sizes and your editor** (`layout.json`,
 * `regions.ts`), under `chats`. It is how one person likes their window, on this machine: a
 * project would carry it to every clone. Each field falls back on its own, as every field of
 * that file does, and a value that is not one is the default's, said in the alerts drawer.
 */
export type ChatsListPrefs = {
  /** How many lines a row is drawn on. One drops what the second line said. */
  lines: 1 | 2;
  /** Whether the sessions are grouped by the workspace they work in. */
  grouped: boolean;
};

export const DEFAULT_CHATS_LIST: ChatsListPrefs = { lines: 2, grouped: false };

/** The preferences as a layout document holds them, and what had to be put right. */
export function loadChatsList(raw: unknown): { prefs: ChatsListPrefs; said: string[] } {
  const said: string[] = [];
  const held =
    raw !== null && typeof raw === "object" && !Array.isArray(raw)
      ? (raw as { chats?: unknown }).chats
      : undefined;
  if (held === undefined) return { prefs: DEFAULT_CHATS_LIST, said };
  if (held === null || typeof held !== "object" || Array.isArray(held)) {
    said.push(
      `"chats" ${JSON.stringify(held)} is not how the Chats list is drawn, so it is the default`,
    );
    return { prefs: DEFAULT_CHATS_LIST, said };
  }
  const from = held as Record<string, unknown>;
  let lines = DEFAULT_CHATS_LIST.lines;
  if (from.lines === 1 || from.lines === 2) lines = from.lines;
  else if (from.lines !== undefined)
    said.push(`"chats.lines" ${JSON.stringify(from.lines)} is not 1 or 2, so it is 2`);
  let grouped = DEFAULT_CHATS_LIST.grouped;
  if (typeof from.grouped === "boolean") grouped = from.grouped;
  else if (from.grouped !== undefined)
    said.push(
      `"chats.grouped" ${JSON.stringify(from.grouped)} is not true or false, so it is false`,
    );
  return { prefs: { lines, grouped }, said };
}

/** Whether `prefs` are the defaults, which the layout file leaves out. */
export function isDefaultChatsList(prefs: ChatsListPrefs): boolean {
  return prefs.lines === DEFAULT_CHATS_LIST.lines && prefs.grouped === DEFAULT_CHATS_LIST.grouped;
}

/** What this launch changed them to, if anything. */
let changed: ChatsListPrefs | undefined;
/** What the launch started from, read once. */
let started: ChatsListPrefs | undefined;
const listeners = new Set<(prefs: ChatsListPrefs) => void>();

function startingChatsList(layout: Reading = atCreation().layout): ChatsListPrefs {
  if (started !== undefined) return started;
  // A layout file purlis refused is said once, by `regions.ts`, and is the defaults here too.
  const { prefs, said } =
    layout.found && layout.trouble === null
      ? loadChatsList(layout.document)
      : { prefs: DEFAULT_CHATS_LIST, said: [] };
  if (said.length > 0) {
    const where = layout.path || "the layout file";
    sayAboutThisMachine("chats", {
      severity: "warn",
      detail: `${where}: ${said.join("; ")}`,
      remedy: `fix ${where}, or change the Chats list in Settings, which rewrites it`,
      settings: "you.chats",
    });
  }
  started = prefs;
  return prefs;
}

/** How the Chats list is drawn now. */
export function chatsListPrefs(): ChatsListPrefs {
  return changed ?? startingChatsList();
}

/** Changes how the Chats list is drawn; tells every listener when it changed. */
export function setChatsListPrefs(to: Partial<ChatsListPrefs>): void {
  const was = chatsListPrefs();
  const now = { ...was, ...to };
  if (now.lines === was.lines && now.grouped === was.grouped) return;
  changed = now;
  sayAboutThisMachine("chats", undefined);
  for (const listener of listeners) listener(now);
}

/** Calls `listener` whenever the preferences change. Answers the way to stop. */
export function onChatsListPrefs(listener: (prefs: ChatsListPrefs) => void): () => void {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

/** {@link chatsListPrefs}, for a component that redraws when they change. */
export function useChatsListPrefs(): ChatsListPrefs {
  return useSyncExternalStore(onChatsListPrefs, chatsListPrefs);
}

/** Forgets what this launch chose and read, as a new launch would. For tests. */
export function forgetChatsListPrefs(): void {
  changed = undefined;
  started = undefined;
}
