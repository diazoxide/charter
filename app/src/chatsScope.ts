import type { ChatRow } from "./chatsTree";
import { forgetProjectViews, keepFacet, keptFacet } from "./projectViews";
import { onLayoutMovedAside } from "./windowprefs";

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

/**
 * **The chats This tab lists, by number** (#1679, #1696): the tab's own set (`tab`, as the
 * window reads it), and each task it held `before` that has left that set while the list still
 * has its row. A task that ends is in no tab's set any more, but its row stays until its
 * finished row is read (`finished.useRowsUntilRead`), as Workspace and All keep it: This tab
 * keeps it too, for exactly as long. Only a task: a task moved to a tab of its own is still
 * listed among its session's tab's chats, so a task leaves a tab's set only by ending.
 *
 * `before` is what this answered for the same tab last, or nothing for a tab just brought
 * forward. **The same set comes back where nothing changed**, so a list drawn from it is not
 * drawn again for nothing.
 */
export function tabSessions(
  tab: readonly ChatRow[],
  before: ReadonlySet<number> | undefined,
  rows: readonly ChatRow[],
): ReadonlySet<number> {
  const now = new Set(tab.map((row) => row.session));
  if (before !== undefined && before.size > 0) {
    const tasks = new Set(rows.filter((row) => row.mode === "task").map((row) => row.session));
    for (const session of before) if (tasks.has(session)) now.add(session);
  }
  if (before !== undefined && before.size === now.size && [...now].every((one) => before.has(one)))
    return before;
  return now;
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

/** Where web storage kept the pick before the layout file did (#1679). Read until the window
 *  moves it into the file ({@link settleScope}), and by nothing after. */
const LEGACY_KEY = "purlis.chats.scope:";

const isScope = (held: unknown): held is Scope => SCOPES.some((one) => one.scope === held);

/** The scope the file (or this launch) keeps for `plane`, or nothing. */
function filed(plane: string): Scope | undefined {
  const held = keptFacet(plane, "chats");
  const scope =
    held !== null && typeof held === "object" && !Array.isArray(held)
      ? (held as { scope?: unknown }).scope
      : undefined;
  return isScope(scope) ? scope : undefined;
}

/** What web storage kept for `plane` before the file did, where it kept a scope. */
function legacy(plane: string): Scope | undefined {
  try {
    const held = globalThis.localStorage.getItem(LEGACY_KEY + plane);
    return isScope(held) ? held : undefined;
  } catch {
    return undefined;
  }
}

/**
 * **The scope the person last picked for `plane`**, or the workspace's (#1679).
 *
 * Kept per project on this machine in `layout.json` v2 (B-11, #1696), under the project's
 * `chats.scope` (`projectViews.ts`): a view's scope is how this person looks at this project. A
 * pick web storage kept before that is read until {@link settleScope} moves it. A value that
 * is not a scope is the default.
 */
export function keptScope(plane: string | undefined): Scope {
  if (plane === undefined) return "workspace";
  return filed(plane) ?? legacy(plane) ?? "workspace";
}

/** Keeps `scope` as the one the person picked for `plane`. The default is kept as nothing. */
export function keepScope(plane: string | undefined, scope: Scope): void {
  if (plane === undefined) return;
  keepFacet(plane, "chats", scope === "workspace" ? undefined : { scope });
  forgetLegacy(plane);
}

/**
 * **Moves the pick web storage kept for `plane` into the layout file, once** (#1696), as
 * `regions.ts` once moved its own key: the file's pick wins where it has one, and the key goes
 * either way, so the two never answer the same question.
 */
export function settleScope(plane: string | undefined): void {
  if (plane === undefined) return;
  const held = legacy(plane);
  if (held !== undefined && filed(plane) === undefined) keepScope(plane, held);
  forgetLegacy(plane);
}

function forgetLegacy(plane: string): void {
  try {
    globalThis.localStorage.removeItem(LEGACY_KEY + plane);
  } catch {
    // No storage: nothing kept there.
  }
}

/** Forgets every project's kept scope, as a machine that never picked one. For tests. */
export function forgetKeptScopes(): void {
  forgetProjectViews();
  forgetEveryLegacy();
}

function forgetEveryLegacy(): void {
  try {
    const keys = Object.keys(globalThis.localStorage).filter((key) => key.startsWith(LEGACY_KEY));
    for (const key of keys) globalThis.localStorage.removeItem(key);
  } catch {
    // No storage: nothing kept.
  }
}

// Use the default layout: every project's scope is the default, web storage's old picks too.
onLayoutMovedAside(forgetEveryLegacy);
