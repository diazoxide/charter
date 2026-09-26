import { useRef } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";

/**
 * Deleting a persona (SI-3): what goes, what stays, and one answer.
 *
 * **The core decides.** `persona_remove` is `charter persona remove` without `--force`: it
 * refuses a persona another one still `extends:` or `uses:`, naming each, and that refusal is
 * drawn here verbatim. The window offers no force — repointing the dependents first is the
 * repair the refusal names, and forcing past it leaves a dangling reference, which is a
 * terminal's decision to make.
 *
 * **No name typed back**, unlike a vault. A persona is a directory in the plane's git history, so
 * a delete is a change to commit, not a destruction; a vault's keychain entries are neither.
 *
 * Radix's `AlertDialog`, for `DeleteWorkspace`'s reasons: Cancel is focused by the primitive,
 * Escape is Cancel, and a stray Return is never what deletes.
 */
export function RemovePersona({
  persona,
  trouble,
  deleting,
  onDelete,
  onCancel,
}: {
  persona: string;
  /** The last delete's refusal, in the core's words. */
  trouble?: string;
  /** Whether a delete is running right now, so the answer cannot be given twice. */
  deleting: boolean;
  onDelete: () => void;
  onCancel: () => void;
}) {
  const cancel = useRef<HTMLButtonElement>(null);
  return (
    <AlertDialog.Root
      open
      onOpenChange={(open) => {
        if (!open && !deleting) onCancel();
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
          <AlertDialog.Title>Delete persona {persona}?</AlertDialog.Title>
          <AlertDialog.Description className="came-back">
            This deletes <code>personas/{persona}/</code> — its definition, its memory and its refs
            — and the agent charter generated for it. Its vault is left alone. Commit the deletion
            to share it.
          </AlertDialog.Description>

          {trouble && (
            <p className="trouble" role="alert">
              {trouble}
            </p>
          )}

          <div className="doing">
            {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). */}
            <button
              type="button"
              className="ends-it"
              tabIndex={0}
              disabled={deleting}
              onClick={onDelete}
            >
              Delete persona
            </button>
            <AlertDialog.Cancel asChild>
              <button type="button" tabIndex={0} ref={cancel} onClick={onCancel}>
                Cancel
              </button>
            </AlertDialog.Cancel>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
