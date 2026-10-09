import type { ChatWorktree } from "./bindings";

/** What a chat's row says about the worktree it is working in.
 *
 *  The branch is the point of the milestone — a chat that writes to a repo is on its own
 *  branch, and the sidebar says which. The two labels beside it are states the operator has
 *  to be able to see, not decoration:
 *
 *  - `unwired` is no longer the ordinary state: since M1.x charter writes the harness layer
 *    into a worktree as it cuts it. What is left is a tree cut by plain git, one whose wire
 *    did not land, and a plane with no layer to carry — and in the first two a chat would
 *    run with **none of the plane's ask/deny rules**, none of its persona's agents, and no
 *    `$CHARTER_HARNESS`. Starting a chat there writes the layer or refuses with a sentence
 *    naming what stopped it, so this label is what the operator sees *before* they click.
 *  - `stale` is a registration whose directory is gone. It is shown rather than cleared on
 *    sight, because clearing a git registration nobody asked charter to touch is not
 *    something to do quietly. */
export function WorktreeMark({
  worktree,
  withBranch = true,
}: {
  worktree: ChatWorktree;
  /** `false` where the row beside the mark already reads the branch (the explorer's branch
   *  rows, #1102), so it is said once. */
  withBranch?: boolean;
}) {
  return (
    <span className="worktree">
      {withBranch && worktree.branch && <code className="branch">{worktree.branch}</code>}
      {worktree.stale && (
        <span
          className="label stale"
          title="git still has this branch's folder registered, but the folder is gone"
        >
          stale
        </span>
      )}
      {!worktree.wired && !worktree.stale && (
        <span
          className="label unwired"
          title="No purlis layer in this branch's folder: no persona agents, no ask/deny rules, no $CHARTER_HARNESS. A harness started here by hand runs without them. Starting a chat from purlis writes the layer, or says why it could not."
        >
          unwired
        </span>
      )}
    </span>
  );
}
