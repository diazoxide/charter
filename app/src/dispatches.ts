import type { DispatchRow, RowWorktree, WorktreeLoss } from "./bindings";
import type { ViewRef } from "./tabs";

/**
 * **Dispatches in the window** (#1452): every dispatch a project's chats made — a handoff today,
 * a task once chats dispatch tasks — with who asked, which persona it went to, where it worked,
 * how it came out, how long it took, how often it needed you and what its harness said it cost.
 *
 * It is a view tab, `{ from: null, view: "dispatches", key: "" }`: one per project, opened from
 * the palette's `dispatches.show` row and from the Sessions panel's heading. The records are the
 * app's own, kept on this machine and never committed, so the tab lists what this machine saw.
 *
 * It is the project's ledger. One workspace's ended tasks, to search and to reopen, are its
 * Past tasks (#1510, `pastTasks.ts`), read from the same records.
 */
export const DISPATCHES_VIEW: ViewRef = { from: null, view: "dispatches", key: "" };

/** What the Dispatches tab is called. */
export const DISPATCHES_TITLE = "Dispatches";

/** Whether `view` is the Dispatches tab. */
export function isDispatches(view: ViewRef): boolean {
  return view.from === null && view.view === DISPATCHES_VIEW.view;
}

/** What the list is narrowed to: a persona, an asking chat, both or neither (`""`). The asking
 *  chat is named by its key (`DispatchRow.asker_key`), never by the name it was called: two
 *  chats called the same are two chats. */
export type DispatchFilter = { persona: string; asker: string };

/** Everything, which is how the tab opens. */
export const EVERY_DISPATCH: DispatchFilter = { persona: "", asker: "" };

/** How a filter names the dispatches that went to no persona. */
export const NO_PERSONA = "\u0000none";

/** What the window says of a dispatch that went to no persona, wherever it says it. */
export const NO_PERSONA_SAID = "No persona";

/** The rows `filter` keeps, in the order they came: newest first. */
export function shownDispatches(
  rows: readonly DispatchRow[],
  filter: DispatchFilter,
): DispatchRow[] {
  return rows.filter(
    (row) =>
      (filter.persona === "" || (row.persona ?? NO_PERSONA) === filter.persona) &&
      (filter.asker === "" || row.asker_key === filter.asker),
  );
}

/** The personas the rows went to, by name; then {@link NO_PERSONA} where a row went to none. */
export function personasOf(rows: readonly DispatchRow[]): string[] {
  const named = [...new Set(rows.flatMap((row) => (row.persona === null ? [] : [row.persona])))];
  named.sort((a, b) => a.localeCompare(b));
  return rows.some((row) => row.persona === null) ? [...named, NO_PERSONA] : named;
}

/** A chat that asked: the key a filter holds it by, and the name it is offered under. */
export type Asker = { key: string; name: string };

/** The chats that asked, each once, by name. A chat is its key: the name is the one its newest
 *  dispatch saw it under. */
export function askersOf(rows: readonly DispatchRow[]): Asker[] {
  const seen = new Map<string, string>();
  for (const row of rows) if (!seen.has(row.asker_key)) seen.set(row.asker_key, row.asker);
  return [...seen]
    .map(([key, name]) => ({ key, name }))
    .sort((a, b) => a.name.localeCompare(b.name) || a.key.localeCompare(b.key));
}

/**
 * **What a row says of the branch its dispatch was given a folder of its own on** (#1453), after
 * where it worked. The window says it of a branch and its folder, never of a worktree
 * (ADR 0072 §4).
 *
 * **A folder that is there is said to be kept, and nothing more.** Whether its branch is merged
 * is not something the row knows: purlis takes a merged branch's folder away only where the
 * folder holds nothing else, so a folder still there may be merged and holding a file.
 */
export function worktreeSaid(worktree: RowWorktree): string {
  switch (worktree.standing) {
    case "kept":
      return "own branch, folder kept";
    case "merged":
      return "own branch, merged and removed";
    case "merged-branch-kept":
      return "own branch, merged; folder removed, branch kept";
    case "discarded":
      return "own branch, folder discarded";
    default:
      return "own branch, folder removed";
  }
}

/** What the question says a discard removes: the folder of the task's own branch. */
export function discardSays(loss: WorktreeLoss): string {
  const of = loss.branch === null ? "" : ` of the branch ${loss.branch}`;
  return `This removes the folder${of} in ${loss.repo}, which purlis cut for ${loss.task}, for good. Nothing is merged.`;
}

/** `1 commit that exists nowhere else`, or `3 commits that exist nowhere else`. */
function unmergedSaid(count: number): string {
  return count === 1
    ? "1 commit that exists nowhere else"
    : `${count} commits that exist nowhere else`;
}

/**
 * What the question says becomes of the branch purlis cut, **which is the record's branch and
 * the only one a discard may delete**, and of what the folder is on now where that is another.
 *
 * - On the branch purlis cut: no commit is lost. The branch is deleted only where git finds it
 *   merged, and one that holds a commit found nowhere else stays.
 * - On another branch (its chat switched the folder): that branch stays with its commits, and
 *   the one purlis cut is deleted only where git finds it merged.
 * - On no branch: nothing keeps the commits made there, so {@link lostSaid} names them as lost.
 */
export function branchSaid(loss: WorktreeLoss): string {
  const cut =
    loss.branch === null
      ? "purlis has no branch on record for it, so no branch is removed."
      : `The branch ${loss.branch}, which purlis cut, is removed only if it is already merged.`;
  if (loss.on === null) return `The folder is on no branch. ${cut}`;
  if (loss.on !== loss.branch) {
    const stays =
      loss.unmerged === 0
        ? `The folder is on the branch ${loss.on}, which stays.`
        : `The folder is on the branch ${loss.on}, which stays and keeps its ${unmergedSaid(loss.unmerged)}.`;
    return `${stays} ${cut}`;
  }
  if (loss.unmerged === 0)
    return `No commit is lost: the branch ${loss.on} is removed only if it is already merged.`;
  return `No commit is lost: the branch ${loss.on} holds ${unmergedSaid(loss.unmerged)}, so it stays.`;
}

/**
 * The heading over the commits a discard **would lose**, or nothing where it loses none: the
 * folder is on no branch and holds commits found nowhere else, which nothing keeps once the
 * folder is gone.
 */
export function lostSaid(loss: WorktreeLoss): string | undefined {
  if (loss.on !== null || loss.unmerged === 0) return undefined;
  return loss.unmerged === 1
    ? "1 commit made on no branch would be lost:"
    : `${loss.unmerged} commits made on no branch would be lost:`;
}

/** Whether a discard would delete nothing: no uncommitted file, no ignored path of the task's,
 *  and no commit made on no branch. */
export function losesNothing(loss: WorktreeLoss): boolean {
  return loss.changes.length === 0 && loss.ignored.length === 0 && lostSaid(loss) === undefined;
}

/** The heading over a list of what goes with the folder: `2 uncommitted files would be lost:`. */
export function counted(count: number, one: string, many: string): string {
  return count === 1 ? `1 ${one}` : `${count} ${many}`;
}

/** A record's time (UTC, RFC 3339) as the window says it: `2026-10-07 12:00 UTC`. The text as
 *  it is where it is not one. */
export function saidAt(stamp: string): string {
  const read = /^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2})/.exec(stamp);
  return read ? `${read[1]} ${read[2]} UTC` : stamp;
}
