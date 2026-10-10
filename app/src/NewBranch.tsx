import { useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { Field, SettingActions, SettingRow } from "./settings/components";

/**
 * **New branch**, from a repo's row (GL-1, ADR 0072 §4).
 *
 * Cuts a branch of its own in the repo — a folder and a branch beside the repo's shared clone,
 * off whatever the clone has checked out — and makes it where the next chat starts. It starts
 * nothing: the next New tab's picker still asks.
 *
 * **It validates nothing**, for `NewWorkspace`'s reason: what a branch may be called is the
 * core's rule, reached through `worktree_add`, which is the path `charter worktree add` takes,
 * so the window refuses exactly what a terminal refuses and says the same sentence. A name left
 * empty is charter's next free `chat-<n>`; a name typed is used exactly, or refused when the
 * repo already has that branch.
 *
 * On screen it says branch and never the words charter keeps for its own internals (ADR 0072
 * §3): the folder is the branch's, and only where a path is needed is it shown.
 */
export function NewBranch({
  repo,
  trouble,
  making,
  onCut,
  onCancel,
}: {
  /** The repo the branch is cut in. */
  repo: string;
  /** Why the last attempt cut nothing — **the core's sentence, unchanged**. */
  trouble?: string;
  /** Whether charter is cutting it right now, so the answer cannot be given twice. */
  making: boolean;
  /** `branch` is the name typed, or `null` for charter's own. */
  onCut: (branch: string | null) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  // The dialog, so the box can be found in it on opening: the row draws the box, and a
  // setting's field takes no ref.
  const content = useRef<HTMLDivElement>(null);
  const cut = () => {
    if (!making) onCut(name.trim() === "" ? null : name.trim());
  };
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content
          ref={content}
          className="warning"
          aria-labelledby="new-branch"
          // A click outside answers nothing, as in every dialog here (`docs/ui-primitives.md`).
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            content.current?.querySelector("input")?.focus();
          }}
        >
          <Dialog.Title id="new-branch">New branch</Dialog.Title>
          <p className="where">
            in <code>{repo}</code>
          </p>

          <form
            onSubmit={(event) => {
              event.preventDefault();
              cut();
            }}
          >
            <SettingRow
              label="Name"
              help={
                <>
                  Optional. Left empty, purlis names it chat-1, chat-2 and on. It is cut from what{" "}
                  <code>{repo}</code> has checked out, in a folder of its own, and new chats start
                  on it until you pick somewhere else.
                </>
              }
              control={(ids) => <Field ids={ids} kind="text" value={name} onChange={setName} />}
            />

            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}

            {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). */}
            <SettingActions>
              <button type="submit" tabIndex={0} disabled={making}>
                {making ? "Creating…" : "Create branch"}
              </button>
              <button type="button" tabIndex={0} onClick={onCancel}>
                Cancel
              </button>
            </SettingActions>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
