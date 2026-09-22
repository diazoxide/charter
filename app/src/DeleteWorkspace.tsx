import { useRef } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import type { AtRisk } from "./bindings";

/**
 * Deleting a workspace: what goes, what charter can see that would be lost with it, and two
 * separate answers rather than one.
 *
 * **The core is what decides, and this surface is careful never to look like it does.**
 * `charter workspace remove` refuses over work that removing the workspace would discard —
 * `wscmd::work_at_risk`: a clone charter could not read, a dirty tree, unpushed commits, a
 * worktree holding commits reachable from no other ref — and that guard runs inside the
 * command, between the name check and `remove_dir_all`. So:
 *
 * - **What this lists is a preview and is drawn as one.** `workspace_at_risk` reads the same
 *   guard so that the operator knows before pressing what charter is going to say. It is not
 *   the decision; the delete asks again, in the core, against the disk at that moment. A
 *   window that treated the preview as the answer would be deciding on a reading taken while
 *   somebody read a dialog.
 * - **The first press never forces.** It runs the delete with `force: false`, and a refusal
 *   comes back in the core's own words.
 * - **Forcing is a second press on a sentence that has been read.** The button does not exist
 *   until the refusal does, and it names what it will discard. This is `worktree.discard`'s
 *   rule in `actions.ts` — *"there is nothing to warn about until the refusal exists, and then
 *   the row appears beside it"* — and it is the reason no catalogue row carries a `force`.
 *
 * Radix's `AlertDialog` rather than `Dialog` (`docs/ui-primitives.md`): it is the primitive for
 * a question whose answer destroys something. It focuses Cancel by itself, it makes Escape
 * mean Cancel, and its content is `role="alertdialog"` — so what a screen reader is handed
 * first is the sentence about what is about to be lost, not a heading and a pair of buttons.
 */
export function DeleteWorkspace({
  workspace,
  /** Everything charter can see that deleting it would discard, or `undefined` while the core
   *  is still being asked. Drawn as "still reading", never as "nothing at risk" — an empty
   *  list and an unanswered question are the two states this dialog must never merge. */
  atRisk,
  /** Why the preview could not be taken, when it could not. Said rather than drawn as an empty
   *  list: "nothing would be lost" is a claim, and this is the absence of one. The delete is
   *  still offered, because the core asks its own question and will refuse on its own answer. */
  unreadable,
  /** The refusal the last delete gave, in the core's words. Its presence is what makes forcing
   *  reachable at all. */
  refusal,
  /** Whether a delete is running right now, so neither answer can be given twice. */
  deleting,
  onDelete,
  onCancel,
}: {
  workspace: string;
  atRisk?: readonly AtRisk[];
  unreadable?: string;
  refusal?: string;
  deleting: boolean;
  onDelete: (force: boolean) => void;
  onCancel: () => void;
}) {
  // Cancel, focused by the primitive itself. Kept as a ref so the reason is written down where
  // somebody might otherwise "tidy" it: this dialog deletes clones, and it is never what a
  // stray Return key finds.
  const cancel = useRef<HTMLButtonElement>(null);
  const risky = atRisk ?? [];
  return (
    <AlertDialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="asking" />
        <AlertDialog.Content
          className="warning"
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            cancel.current?.focus();
          }}
        >
          <AlertDialog.Title>Delete workspace {workspace}?</AlertDialog.Title>
          <AlertDialog.Description className="came-back">
            This deletes <code>workspaces/{workspace}/</code> and everything in it: every repo
            cloned there, every worktree cut in it, its memory and its todos. There is no undo.
          </AlertDialog.Description>

          {/* Still reading. Said rather than drawn as an empty list: "nothing is at risk" is a
              claim about every clone in the workspace, and charter has not made it yet. */}
          {atRisk === undefined && unreadable === undefined && (
            <p className="pending">Asking git what is in it…</p>
          )}

          {unreadable !== undefined && (
            <p className="trouble" role="alert">
              charter could not read what is in it before asking: {unreadable}. Delete still checks
              — the refusal you would get is the one that decides.
            </p>
          )}

          {atRisk !== undefined && risky.length === 0 && (
            <p className="came-back">
              charter found no uncommitted or unpushed work in it. It checks again when you press
              Delete, against the workspace as it is then.
            </p>
          )}

          {risky.length > 0 && (
            <>
              <h3>What charter would discard</h3>
              {/* The core's sentences, one per row, and never a count. `svc: 2 unpushed
                  commit(s)` is what a terminal shows and what the refusal will repeat. */}
              <ul className="at-risk" data-testid="at-risk">
                {risky.map((risk) => (
                  <li key={risk.what}>{risk.said}</li>
                ))}
              </ul>
            </>
          )}

          {/* The refusal, verbatim. It names the repair — push or commit first — and an
              operator shown a reworded version of it can neither follow that repair nor search
              for the sentence. */}
          {refusal && (
            <p className="trouble" role="alert">
              {refusal}
            </p>
          )}

          <div className="doing">
            {/* **Before a refusal there is one answer, and it does not force.** After one there
                is a different answer, and it says what it costs. Never both: a dialog offering
                "Delete" beside "Delete anyway" is offering to force to somebody who has read
                nothing. */}
            {refusal === undefined ? (
              <button
                type="button"
                className="ends-it"
                disabled={deleting}
                onClick={() => onDelete(false)}
              >
                Delete workspace
              </button>
            ) : (
              <button
                type="button"
                className="ends-it"
                disabled={deleting}
                onClick={() => onDelete(true)}
              >
                {discarding(risky)}
              </button>
            )}
            <AlertDialog.Cancel asChild>
              <button type="button" ref={cancel} onClick={onCancel}>
                Cancel
              </button>
            </AlertDialog.Cancel>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}

/**
 * What the force button says, and it names what goes rather than saying "anyway".
 *
 * A count and the names, because both are the question: *how much* is being thrown away, and
 * *which* of the things on the list it is. With nothing on the list — the core refused over
 * something the preview did not see, which is what a workspace changing under a dialog looks
 * like — the button says so instead of naming nothing.
 */
function discarding(risky: readonly AtRisk[]): string {
  if (risky.length === 0) return "Delete it anyway, discarding what charter just refused over";
  const names = risky.map((risk) => risk.what).join(", ");
  return risky.length === 1
    ? `Delete it anyway, discarding the work in ${names}`
    : `Delete it anyway, discarding the work in ${risky.length}: ${names}`;
}
