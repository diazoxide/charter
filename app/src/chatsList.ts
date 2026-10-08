/**
 * **The Chats list at fifty chats** (#1499, V100-47 to V100-50): the order its sessions stand
 * in, which of them fold by themselves, and what a filter leaves drawn.
 *
 * Pure, and read off the tree `chatsTree.ts` shapes. Each answer here depends on what the chats
 * are doing, so the section asks for each through the store's selector and is drawn again only
 * when an answer changes: a chat that moves without changing the order, a fold or the filter's
 * answer redraws its own marks and no row (SC-3).
 */
import type { FinishedTask } from "./bindings";
import { markOf, type ChatStates } from "./chatState";
import type { ChatRow } from "./chatsTree";
import { shownOf } from "./finished";
import { shownState, type Shown, type ShownKind, type ShownShape } from "./shownState";

/** A row's state as `ChatShownState` draws it: the one function, on the row's own facts. */
export function shownOfRow(
  states: ChatStates,
  row: ChatRow,
  /** The needs-you queue as a set, for a caller that asks about every row. */
  queue: ReadonlySet<number> = new Set(states.needsYou),
): Shown | undefined {
  return shownState({
    board: markOf(states, row.session, row.shell),
    needsYou: queue.has(row.session),
    task:
      row.report === null ? null : { report: row.report, outcome: row.outcome, asking: row.asking },
    harness: row.harness,
  });
}

/** Where a state stands in the list's order: needs you, then at work, then everything else. */
export type Rank = 0 | 1 | 2;

/**
 * The rank of a state (V100-47). A task asking the chat that dispatched it, and a chat the
 * board has heard nothing from, are at work: their programs run and nothing waits on the
 * person.
 */
export function rankOf(kind: ShownKind | undefined): Rank {
  if (kind === "needs-you") return 0;
  if (kind === "working" || kind === "asking" || kind === "unheard") return 1;
  return 2;
}

/** The states a chat's row keeps once it is over: nothing more will come of it. */
const OVER: ReadonlySet<ShownKind> = new Set([
  "done",
  "failed",
  "cancelled",
  "unreported",
  "reported",
]);

/** Whether a state is one a chat is still in: at work, idle, or needing someone. */
export function isLive(kind: ShownKind | undefined): boolean {
  return kind === undefined || !OVER.has(kind);
}

/** Row `at`'s rows below it, at any depth: the index after the last of them. */
function endOf(rows: readonly ChatRow[], at: number): number {
  let next = at + 1;
  while (next < rows.length && rows[next].level > rows[at].level) next += 1;
  return next;
}

/**
 * **The sessions in the order they are listed** (V100-47), by number: the ones that need the
 * person, then the ones at work, then the rest, each in the order it was started.
 *
 * **A session stands where the most urgent chat under it would**: a session whose task needs
 * the person is one the person has to go to, and it wears the hand for it.
 *
 * With `grouped`, the sessions of one workspace stand together, the workspaces by name, and
 * the same order holds inside each.
 */
export function sessionOrder(
  rows: readonly ChatRow[],
  kindOf: (row: ChatRow) => ShownKind | undefined,
  grouped: boolean,
): number[] {
  const tops: { session: number; workspace: string; rank: Rank; at: number }[] = [];
  for (let at = 0; at < rows.length; at = endOf(rows, at)) {
    let rank: Rank = 2;
    for (let under = at; under < endOf(rows, at); under += 1) {
      const one = rankOf(kindOf(rows[under]));
      if (one < rank) rank = one;
    }
    tops.push({ session: rows[at].session, workspace: rows[at].workspace, rank, at });
  }
  return tops
    .sort(
      (one, other) =>
        (grouped ? one.workspace.localeCompare(other.workspace) : 0) ||
        one.rank - other.rank ||
        one.at - other.at,
    )
    .map((top) => top.session);
}

/**
 * The rows with their sessions in `order`, each with everything under it.
 *
 * **A session the order does not name is put where it moves the fewest rows**: an order held
 * still does not know a session that came to the top after it was taken. One that arrived
 * stands after every session the order names. One whose parent closed was already drawn, under
 * that parent, so it stands in its parent's place where the order names it, and otherwise
 * straight after the session started before it, which is the one it was drawn under.
 */
export function arranged(rows: readonly ChatRow[], order: readonly number[]): ChatRow[] {
  const named = new Map(order.map((session, at) => [session, at]));
  const tops: { at: number; end: number; place: number }[] = [];
  for (let at = 0; at < rows.length; at = endOf(rows, at)) {
    const row = rows[at];
    const before = tops[tops.length - 1]?.place;
    const place =
      named.get(row.session) ??
      (row.orphaned && row.parent !== null ? (named.get(row.parent) ?? before) : undefined) ??
      order.length;
    tops.push({ at, end: endOf(rows, at), place });
  }
  // Already in order, which is every draw but the one that moves a session: the same rows.
  if (tops.every((top, at) => at === 0 || tops[at - 1].place <= top.place)) return [...rows];
  return tops
    .sort((one, other) => one.place - other.place || one.at - other.at)
    .flatMap((top) => rows.slice(top.at, top.end));
}

/**
 * The rows, each saying where it stands among the rows drawn beside it under the same row:
 * `posinset` and `setsize` as a screen reader is told them, after an order or a filter has
 * changed which rows those are.
 */
export function stamped(rows: readonly ChatRow[]): ChatRow[] {
  /** Each row's parent, by index; -1 at the top. */
  const parent: number[] = [];
  const size = new Map<number, number>();
  /** The last row seen at each level, by index. */
  const last: number[] = [];
  rows.forEach((row, at) => {
    last.length = row.level;
    last[row.level - 1] = at;
    const under = row.level > 1 ? (last[row.level - 2] ?? -1) : -1;
    parent.push(under);
    size.set(under, (size.get(under) ?? 0) + 1);
  });
  const seen = new Map<number, number>();
  return rows.map((row, at) => {
    const posinset = (seen.get(parent[at]) ?? 0) + 1;
    seen.set(parent[at], posinset);
    const setsize = size.get(parent[at]) ?? 1;
    return row.posinset === posinset && row.setsize === setsize
      ? row
      : { ...row, posinset, setsize };
  });
}

/**
 * **The sessions that are open by themselves** (V100-48): the ones with a chat under them, at
 * any depth, that is still at work, idle or needing someone. A session whose chats are all
 * over is not here, and folds by itself.
 */
export function liveBelow(
  rows: readonly ChatRow[],
  kindOf: (row: ChatRow) => ShownKind | undefined,
): number[] {
  const live: number[] = [];
  rows.forEach((row, at) => {
    for (let under = at + 1; under < endOf(rows, at); under += 1) {
      if (isLive(kindOf(rows[under]))) {
        live.push(row.session);
        return;
      }
    }
  });
  return live;
}

/** What the filter asks for. */
export type Filter = {
  /** What was typed: every word of it must be in the row's name, persona, workspace or state. */
  text: string;
  /**
   * The chips pressed: a row whose state stands at any of these ranks. **By rank and not by
   * word**, so the "working" chip finds what the order puts with the working: a task asking
   * its asker, and a chat on a harness purlis hears nothing from.
   */
  ranks: readonly Rank[];
};

/** Whether a filter asks for anything. */
export function filters(filter: Filter): boolean {
  return filter.text.trim() !== "" || filter.ranks.length > 0;
}

/** Text as the filter compares it: case folded and accents taken off, so "jose" finds "José". */
function plain(text: string): string {
  return text.normalize("NFKD").replace(/\p{M}/gu, "").toLocaleLowerCase();
}

/** Whether every word typed is in one of `fields`. */
function inFields(text: string, fields: readonly string[]): boolean {
  const among = fields.map(plain);
  return plain(text)
    .split(/\s+/)
    .filter((word) => word !== "")
    .every((word) => among.some((field) => field.includes(word)));
}

/** Whether `row`, in the state `shown`, is one the filter asks for (V100-49). */
export function matches(row: ChatRow, shown: Shown | undefined, filter: Filter): boolean {
  if (filter.ranks.length > 0 && !filter.ranks.includes(rankOf(shown?.kind))) return false;
  return inFields(filter.text, [row.name, row.persona ?? "", row.workspace, shown?.word ?? ""]);
}

/**
 * Whether a finished task is one the filter asks for: by its name, persona, the place it
 * worked in, and how it ended, in the row's word and in the core's. Never by a chip: both ask
 * for a chat that is still running.
 */
export function matchesFinished(task: FinishedTask, filter: Filter): boolean {
  if (filter.ranks.length > 0 || filter.text.trim() === "") return false;
  return inFields(filter.text, [
    task.name,
    task.persona ?? "",
    task.place,
    shownOf(task)?.word ?? "",
    task.outcome,
  ]);
}

/**
 * **The rows a filter leaves drawn**: the ones it asks for, and every row above one of them,
 * so a task that matches is still drawn under the session that asked for it.
 */
export function found(rows: readonly ChatRow[], asked: ReadonlySet<number>): ChatRow[] {
  const kept: ChatRow[] = [];
  /** The rows above the one being read, nearest last, and whether each is kept yet. */
  const above: { row: ChatRow; kept: boolean }[] = [];
  for (const row of rows) {
    above.length = row.level - 1;
    const here = { row, kept: false };
    if (asked.has(row.session)) {
      for (const up of above) {
        // A level the lineage skipped leaves a hole here, which holds no row.
        if (up === undefined || up.kept) continue;
        up.kept = true;
        kept.push(up.row);
      }
      here.kept = true;
      kept.push(row);
    }
    above[row.level - 1] = here;
  }
  // An ancestor is kept when its first match is read, which is after rows of other sessions
  // may have been: back in the order they came.
  const place = new Map(rows.map((row, at) => [row.session, at]));
  return kept.sort((one, other) => (place.get(one.session) ?? 0) - (place.get(other.session) ?? 0));
}

/** The kinds a folded session counts its finished tasks by, in the order it says them. */
const COUNTED: readonly ShownKind[] = ["done", "cancelled", "failed", "unreported", "reported"];

/** One count of a folded session's summary: how many of its tasks ended one way. */
export type Counted = { shape: ShownShape; count: number; word: string };

/**
 * **What a folded session says of its finished tasks** (V100-48, "steward 4 · ✓5"): how many
 * ended each way, each as the shape and the word that state has on a row, or nothing where it
 * has none. One plain value (`shape:count:word`, joined by `|`), so a row held on it is drawn
 * again only when a count changes.
 */
export function summaryOf(tasks: readonly FinishedTask[]): string | null {
  const counts = new Map<ShownKind, { shown: Shown; count: number }>();
  for (const task of tasks) {
    const shown = shownOf(task);
    if (shown === undefined) continue;
    counts.set(shown.kind, { shown, count: (counts.get(shown.kind)?.count ?? 0) + 1 });
  }
  const said = COUNTED.flatMap((kind) => {
    const one = counts.get(kind);
    return one === undefined ? [] : [`${one.shown.shape}:${one.count}:${one.shown.word}`];
  });
  return said.length === 0 ? null : said.join("|");
}

/** A summary as its counts, in the order it says them. */
export function countsOf(summary: string): Counted[] {
  return summary.split("|").map((one) => {
    const [shape, count, ...word] = one.split(":");
    return { shape: shape as ShownShape, count: Number(count), word: word.join(":") };
  });
}

/** How long a chat has been in its state, as its row says it. */
export function sinceSaid(seconds: number): string {
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)}h`;
  return `${Math.floor(seconds / 86_400)}d`;
}
