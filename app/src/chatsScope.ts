import type { ChatRow } from "./chatsTree";

/**
 * **The Chats list follows the focused workspace** (#1655, D-qw97-1): the rows of the trees
 * that started there, each with everything below it.
 *
 * A tree belongs to where its top row works: the chat a person opened, or a handoff, which
 * moved the work to where it now runs. A task stays under the chat that asked for it wherever
 * it works, so a tree is never cut in two (#1447). A chat started at the plane root belongs to
 * the root's view, by the word its rows say for it (D-qw97-2).
 *
 * The rows come back as they are where nothing is left out, so a list drawn from them is not
 * drawn again for nothing; otherwise the top rows are counted again among themselves, as a
 * screen reader reads them.
 */
export function inScope(rows: readonly ChatRow[], here: string): readonly ChatRow[] {
  const kept: ChatRow[] = [];
  let keeping = false;
  for (const row of rows) {
    if (row.level === 1) keeping = row.workspace === here;
    if (keeping) kept.push(row);
  }
  if (kept.length === rows.length) return rows;
  const tops = kept.filter((row) => row.level === 1).length;
  let at = 0;
  return kept.map((row) =>
    row.level === 1 ? { ...row, posinset: (at += 1), setsize: tops } : row,
  );
}

/**
 * **The rows of the chats a tab holds** (#1679): `sessions` is the tab's set as the window
 * already reads it (`tabChats.chatsOfTabs`), in the order and nesting the whole list has. A
 * row whose asker the tab does not hold stands at the top, with the tab's rows below it: a
 * task in a tab of its own heads that tab's list.
 *
 * Every level is counted again among its own, as a screen reader reads them, and the rows
 * come back as they are where nothing is left out.
 */
export function inTab(rows: readonly ChatRow[], sessions: ReadonlySet<number>): readonly ChatRow[] {
  /** By level in the whole list, the level the nearest row the tab holds, at or above that
   *  one on the way down to this row, is drawn at: 0 where there is none. */
  const drawnAt: number[] = [];
  const kept: ChatRow[] = [];
  for (const row of rows) {
    drawnAt.length = row.level - 1;
    const above = drawnAt[row.level - 2] ?? 0;
    const holds = sessions.has(row.session);
    drawnAt.push(holds ? above + 1 : above);
    if (holds) kept.push(above + 1 === row.level ? row : { ...row, level: above + 1 });
  }
  if (kept.length === rows.length) return rows;
  return counted(kept);
}

/** `rows` with each one's place among the rows at its level under the same row above. */
function counted(rows: readonly ChatRow[]): ChatRow[] {
  const sizes = new Map<number, number>();
  /** The group each row is in: the index of the row it is under, -1 for the top. */
  const groups: number[] = [];
  const under: number[] = [];
  rows.forEach((row, at) => {
    under.length = row.level - 1;
    const group = row.level === 1 ? -1 : under[row.level - 2];
    groups.push(group);
    sizes.set(group, (sizes.get(group) ?? 0) + 1);
    under.push(at);
  });
  const placed = new Map<number, number>();
  return rows.map((row, at) => {
    const group = groups[at];
    const posinset = (placed.get(group) ?? 0) + 1;
    placed.set(group, posinset);
    const setsize = sizes.get(group) ?? 1;
    return row.posinset === posinset && row.setsize === setsize
      ? row
      : { ...row, posinset, setsize };
  });
}

/**
 * **Which chats the Chats view lists** (#1679, B-15): the tab in front's, the focused
 * workspace's (the default, #1655), or every workspace's. Whichever it is, a chat it leaves
 * out that needs the person is named at the top, with a way to it.
 */
export type Scope = "tab" | "workspace" | "all";

/** The scopes, as the switch says them, narrowest first. */
export const SCOPES: readonly { scope: Scope; says: string }[] = [
  { scope: "tab", says: "This tab" },
  { scope: "workspace", says: "Workspace" },
  { scope: "all", says: "All" },
];

const KEY = "purlis.chats.scope:";

/**
 * **The scope the person last picked for `plane`**, or the workspace's (#1679).
 *
 * Kept in the window's web storage, by project, so it survives a relaunch and each project
 * has its own: a view's scope is how this person looks at this project on this machine, and no
 * file holds it yet (ADR 0038, 2026-10-10). Nothing here is needed: storage a webview refuses, or a value
 * that is not a scope, is the default.
 */
export function keptScope(plane: string | undefined): Scope {
  if (plane === undefined) return "workspace";
  try {
    const held = globalThis.localStorage.getItem(KEY + plane);
    return SCOPES.find((one) => one.scope === held)?.scope ?? "workspace";
  } catch {
    return "workspace";
  }
}

/** Keeps `scope` as the one the person picked for `plane`. The default is kept as no value. */
export function keepScope(plane: string | undefined, scope: Scope): void {
  if (plane === undefined) return;
  try {
    if (scope === "workspace") globalThis.localStorage.removeItem(KEY + plane);
    else globalThis.localStorage.setItem(KEY + plane, scope);
  } catch {
    // Storage refused: the pick holds for this window's run, as #1655's chip did.
  }
}

/** Forgets every project's kept scope, as a machine that never picked one. For tests. */
export function forgetKeptScopes(): void {
  try {
    const keys = Object.keys(globalThis.localStorage).filter((key) => key.startsWith(KEY));
    for (const key of keys) globalThis.localStorage.removeItem(key);
  } catch {
    // No storage: nothing kept.
  }
}
