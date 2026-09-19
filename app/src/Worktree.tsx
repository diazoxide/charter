import { useState } from "react";
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
export function WorktreeMark({ worktree }: { worktree: ChatWorktree }) {
  return (
    <span className="worktree">
      {worktree.branch && <code className="branch">{worktree.branch}</code>}
      {worktree.stale && (
        <span
          className="label stale"
          title="git still has this worktree registered, but its directory is gone"
        >
          stale
        </span>
      )}
      {!worktree.wired && !worktree.stale && (
        <span
          className="label unwired"
          title="No charter layer in this worktree: no persona agents, no ask/deny rules, no $CHARTER_HARNESS. A harness started here by hand runs without them. Starting a chat from charter writes the layer, or says why it could not."
        >
          unwired
        </span>
      )}
    </span>
  );
}

/** Removing a piece, with the core's refusal in front of the operator.
 *
 *  The whole reason this verb is in the window rather than only in the terminal is that the
 *  guards have something to say. `charter_core::worktree::remove` refuses a tree with
 *  uncommitted changes, a tree it could not read, and a branch holding commits that exist
 *  nowhere else — each with a sentence naming the repair. Those sentences are shown here
 *  **verbatim**: the window does not reword them, and does not replace them with a generic
 *  failure.
 *
 *  `--force` is how the operator says to discard that work. It is deliberately a second
 *  decision, offered only after the refusal has been read, and it is never sent on the
 *  operator's behalf. */
export function RemovePiece({
  worktree,
  plane,
  onRemove,
  onRemoved,
}: {
  worktree: ChatWorktree;
  plane: string;
  onRemove: (args: {
    plane: string;
    workspace: string;
    repo: string;
    piece: string;
    force: boolean;
  }) => Promise<{ status: "ok" } | { status: "error"; error: string }>;
  onRemoved?: () => void;
}) {
  const [refusal, setRefusal] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function ask(force: boolean) {
    setBusy(true);
    const answer = await onRemove({
      plane,
      workspace: worktree.workspace,
      repo: worktree.repo,
      piece: worktree.piece,
      force,
    });
    setBusy(false);
    if (answer.status === "ok") {
      setRefusal(null);
      onRemoved?.();
      return;
    }
    setRefusal(answer.error);
  }

  return (
    <div className="remove-piece" data-testid={`remove-${worktree.piece}`}>
      <button disabled={busy} onClick={() => ask(false)}>
        Remove worktree
      </button>
      <p className="keeps-the-branch">
        The worktree goes; the branch <code>{worktree.branch ?? worktree.piece}</code> stays.
      </p>

      {refusal && (
        <div className="refusal" role="alert">
          {/* Verbatim. The sentence names the repair, and an operator who is shown a
              reworded version of it cannot follow that repair or search for it. */}
          <p className="said">{refusal}</p>
          <button className="discard" disabled={busy} onClick={() => ask(true)}>
            Discard it anyway
          </button>
        </div>
      )}
    </div>
  );
}
