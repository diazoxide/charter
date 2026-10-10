import { layoutPref } from "./layoutPref";

/**
 * **How the Chats list is drawn** (#1499, V100-73): whether the sessions of one workspace
 * stand together. A row is one line (#1675), so how many lines it takes is no longer a choice:
 * a file that still says `lines` is read as if it did not, and says nothing of it. **And what pressing a task in it does**
 * (#1489, V100-74): shows it inside its session's tab, or opens it in a tab of its own.
 *
 * **Kept in the layout file, beside the text sizes and your editor** (`layout.json`,
 * `regions.ts`), under `chats`. It is how one person likes their window, on this machine: a
 * project would carry it to every clone. Each field falls back on its own, as every field of
 * that file does, and a value that is not one is the default's, said in the alerts drawer.
 */
export type ChatsListPrefs = {
  /** Whether the sessions are grouped by the workspace they work in. */
  grouped: boolean;
  /** Whether a pressed task opens in a tab of its own (V100-74, "Open tasks in their own
   *  tabs"), where otherwise its session's tab is switched to it. */
  tabbed: boolean;
  /** Whether coming back to the window after a while away sums up what happened meanwhile
   *  (#1514, V100-73, "Away summary"), one Notice per project (`AwaySummary.tsx`). */
  away: boolean;
};

export const DEFAULT_CHATS_LIST: ChatsListPrefs = {
  grouped: false,
  tabbed: false,
  away: true,
};

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
  let grouped = DEFAULT_CHATS_LIST.grouped;
  if (typeof from.grouped === "boolean") grouped = from.grouped;
  else if (from.grouped !== undefined)
    said.push(
      `"chats.grouped" ${JSON.stringify(from.grouped)} is not true or false, so it is false`,
    );
  let tabbed = DEFAULT_CHATS_LIST.tabbed;
  if (typeof from.tabbed === "boolean") tabbed = from.tabbed;
  else if (from.tabbed !== undefined)
    said.push(`"chats.tabbed" ${JSON.stringify(from.tabbed)} is not true or false, so it is false`);
  let away = DEFAULT_CHATS_LIST.away;
  if (typeof from.away === "boolean") away = from.away;
  else if (from.away !== undefined)
    said.push(`"chats.away" ${JSON.stringify(from.away)} is not true or false, so it is true`);
  return { prefs: { grouped, tabbed, away }, said };
}

/** Whether `prefs` are the defaults, which the layout file leaves out. */
export function isDefaultChatsList(prefs: ChatsListPrefs): boolean {
  return (
    prefs.grouped === DEFAULT_CHATS_LIST.grouped &&
    prefs.tabbed === DEFAULT_CHATS_LIST.tabbed &&
    prefs.away === DEFAULT_CHATS_LIST.away
  );
}

/** The store (`layoutPref.ts`), under `chats`. */
export const CHATS_LIST = layoutPref<ChatsListPrefs>({
  key: "chats",
  fallback: DEFAULT_CHATS_LIST,
  load: (raw) => {
    const { prefs, said } = loadChatsList(raw);
    return { value: prefs, said };
  },
  same: (one, other) =>
    one.grouped === other.grouped && one.tabbed === other.tabbed && one.away === other.away,
  written: (prefs) => (isDefaultChatsList(prefs) ? undefined : prefs),
  remedy: (where) => `fix ${where}, or change the Chats list in Settings, which rewrites it`,
  settings: "you.chats",
});

/** How the Chats list is drawn now. */
export function chatsListPrefs(): ChatsListPrefs {
  return CHATS_LIST.value();
}

/** Changes how the Chats list is drawn; tells every listener when it changed. */
export function setChatsListPrefs(to: Partial<ChatsListPrefs>): void {
  CHATS_LIST.set({ ...chatsListPrefs(), ...to });
}

/** {@link chatsListPrefs}, for a component that redraws when they change. */
export function useChatsListPrefs(): ChatsListPrefs {
  return CHATS_LIST.use();
}
