import type { BranchMerge, ChangedIn, OwnBranch, TaskChanges } from "./bindings";
import type { Place } from "./pieceViews";
import type { ViewRef } from "./tabs";

/**
 * **What a task changed, in a view tab of its own** (#1511, V100-66): `{ from: null, view:
 * "task-changes", key: <the dispatch's id> }`, one per task, opened from **Changes** on a
 * finished task's row and from its report's line about what changed.
 *
 * The key is the dispatch's id and nothing else: the core reads which folder, repo and branch
 * from its own record of that dispatch, so the window never names a path or a branch to merge.
 */
const TASK_CHANGES = "task-changes";

/** The Changes tab of the task of dispatch `id`. */
export function taskChangesView(id: string): ViewRef {
  return { from: null, view: TASK_CHANGES, key: id };
}

/** What a task's Changes tab is called. */
export function taskChangesTitle(task: string): string {
  return `Changes · ${task}`;
}

/** The dispatch a view is a task's Changes tab of; `undefined` for every other view. */
export function taskChangesOf(view: ViewRef): string | undefined {
  return view.from === null && view.view === TASK_CHANGES && view.key !== "" ? view.key : undefined;
}

/** The view kind, for the window's list of its own views' marks. */
export const TASK_CHANGES_VIEW = TASK_CHANGES;

/** Where the files of one place are, as the comparison views name a branch's files. */
export function placeOf(changed: ChangedIn): Place {
  return { workspace: changed.workspace, repo: changed.repo, piece: changed.piece };
}

/** What a place is called on the tab: the branch's folder in its repo, or the repo. */
export function placeSaid(changed: ChangedIn): string {
  return changed.piece === null ? changed.repo : `${changed.piece} in ${changed.repo}`;
}

/** What the tab says of a task's own branch, by how it stands. */
export function ownSaid(own: OwnBranch): string {
  const branch = own.branch ?? "its own branch";
  switch (own.standing) {
    case "kept":
      return `It worked on its own branch, ${branch} in ${own.repo}. What is listed is everything that branch changed since it was cut.`;
    case "merged":
    case "merged-branch-kept":
      return `Its own branch, ${branch} in ${own.repo}, is merged.`;
    case "discarded":
      return `The folder of its own branch, ${branch} in ${own.repo}, was discarded.`;
    default:
      return `The folder of its own branch, ${branch} in ${own.repo}, is gone.`;
  }
}

/** What the tab says above a task's files, where it worked in a folder other chats work in. */
export const SHARED_SAID =
  "It worked in a folder other chats work in. Listed are the files its own tools named that git finds changed there, and nothing else of that folder's.";

/** How many files a task's changes list, across every place. */
export function filesIn(changes: TaskChanges): number {
  return changes.places.reduce((sum, place) => sum + place.files.length, 0);
}

/** `1 commit`, or `3 commits`. */
function commits(count: number): string {
  return count === 1 ? "1 commit" : `${count} commits`;
}

/**
 * What the merge question says will happen, or why it will not: purlis lands the branch in the
 * branch it was cut from as a fast-forward, or does nothing.
 */
export function mergeSays(merge: BranchMerge): string {
  const into = merge.into ?? "the branch it was cut from";
  return `This lands ${commits(merge.ahead)} of ${merge.branch}, which purlis cut for ${merge.task}, in ${into} in ${merge.repo}. purlis only fast-forwards: it writes no merge commit and never forces one.`;
}

/**
 * Why the merge will be refused, where the question can already see it, or nothing. The core
 * refuses each of these itself; saying it first spares a press that would change nothing.
 */
export function mergeBlocked(merge: BranchMerge): string | undefined {
  if (merge.into === null)
    return "purlis has no record of the branch this one was cut from, so it does not know where to merge it.";
  if (merge.uncommitted.length > 0)
    return "Its folder holds changes that are not committed, so purlis will not merge it. Ask the task, or commit them yourself, first.";
  if (merge.behind > 0)
    return `${merge.into} has ${commits(merge.behind)} the task's branch does not have, so the branch no longer fast-forwards. Merge ${merge.into} into ${merge.branch} where its conflicts belong, then merge again.`;
  if (merge.ahead === 0) return `${merge.branch} has nothing to land in ${merge.into}.`;
  return undefined;
}
