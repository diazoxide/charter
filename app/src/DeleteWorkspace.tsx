import { useRef } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import type { AtRisk, Refused } from "./bindings";

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
 * - **And it names it out of the refusal, never out of the preview** (charter-app#182). Once
 *   there has been an attempt, the list above and the words on the button both come from
 *   `refusal.at_risk` — the reading the core made *inside* the delete. The preview is what is
 *   drawn before the first press and nothing after it. A workspace can change while a dialog
 *   is open, and a button naming `svc` under a sentence naming `lib` is two descriptions of
 *   one act that do not agree, at the moment consent is given.
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
  /** The refusal the last delete gave: the core's words, **and the at-risk list it refused
   *  on**. Its presence is what makes forcing reachable at all, and its list is what the
   *  force button says once there is one (charter-app#182). */
  refusal,
  /** Whether a delete is running right now, so neither answer can be given twice. */
  deleting,
  onDelete,
  onCancel,
}: {
  workspace: string;
  atRisk?: readonly AtRisk[];
  unreadable?: string;
  refusal?: Refused;
  deleting: boolean;
  onDelete: (force: boolean) => void;
  onCancel: () => void;
}) {
  // Cancel, focused by the primitive itself. Kept as a ref so the reason is written down where
  // somebody might otherwise "tidy" it: this dialog deletes clones, and it is never what a
  // stray Return key finds.
  const cancel = useRef<HTMLButtonElement>(null);
  /**
   * What is about to be lost, as the newest reading of the guard says it.
   *
   * **The refusal's list wins the moment there is one** (charter-app#182). `atRisk` was read
   * when this dialog opened; `refusal.at_risk` was read inside the delete, after the operator
   * finished reading. They are the same guard at two moments, and only the second is about the
   * workspace as it is now — so drawing the older one beside the newer one's sentence is how
   * the list and the sentence come to name different clones.
   */
  const risky = refusal?.at_risk ?? atRisk ?? [];
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

          {/* The three lines below are **about the preview**, so they stop being drawn the
              moment there has been an attempt: after one, what is on screen is the core's own
              reading and a sentence about how the guess was taken is noise at best and a
              contradiction at worst — "charter found no uncommitted work in it" above a
              refusal listing two dirty clones (charter-app#182). */}

          {/* Still reading. Said rather than drawn as an empty list: "nothing is at risk" is a
              claim about every clone in the workspace, and charter has not made it yet. */}
          {refusal === undefined && atRisk === undefined && unreadable === undefined && (
            <p className="pending">Asking git what is in it…</p>
          )}

          {refusal === undefined && unreadable !== undefined && (
            <p className="trouble" role="alert">
              charter could not read what is in it before asking: {unreadable}. Delete still checks
              — the refusal you would get is the one that decides.
            </p>
          )}

          {refusal === undefined && atRisk !== undefined && risky.length === 0 && (
            <p className="came-back">
              charter found no uncommitted or unpushed work in it. It checks again when you press
              Delete, against the workspace as it is then.
            </p>
          )}

          {risky.length > 0 && (
            <>
              <h3>What charter would discard</h3>
              {/* The core's sentences, one per row, and never a count. `svc: 2 unpushed
                  commit(s)` is what a terminal shows and what the refusal repeats — and once
                  there has been a refusal these rows ARE the refusal's own list. */}
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
              {refusal.said}
            </p>
          )}

          <div className="doing">
            {/* **Before a refusal there is one answer, and it does not force.** After one there
                is a different answer, and it says what it costs. Never both: a dialog offering
                "Delete" beside "Delete anyway" is offering to force to somebody who has read
                nothing.
                **And forcing is offered only when the guard is what refused**, which is exactly
                when the refusal carries a list (charter-app#182). A name that is not a
                workspace, a `workspaces/<ws>` that links out of the plane, a `remove_dir_all`
                that failed — `--force` gets past none of them, and a button offering to force
                past one would be a lie about what the next press does. The plain answer stays
                there instead, because retrying is the only thing that could help. */}
            {refusal === undefined || risky.length === 0 ? (
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
 * *which* of the things on the list it is.
 *
 * **It is never handed an empty list, and that is the fix charter-app#182 bought.** The words
 * used to be drawn from the preview, so a workspace that changed while the dialog was up could
 * leave this naming nothing — there was a fallback sentence for exactly that — or, worse,
 * naming the wrong clone under a refusal that named another. The list now comes from the
 * refusal itself, and the guard refuses only when it found something, so the caller draws this
 * button only where there is something to name.
 */
function discarding(risky: readonly AtRisk[]): string {
  const names = risky.map((risk) => risk.what).join(", ");
  return risky.length === 1
    ? `Delete it anyway, discarding the work in ${names}`
    : `Delete it anyway, discarding the work in ${risky.length}: ${names}`;
}
