import { useId, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";

/**
 * Renaming a workspace (charter#367), asked where the answer is given.
 *
 * **This dialog validates nothing**, for `NewWorkspace`'s reason: what a workspace may be
 * called, whether the name is taken, and whether a chat is running in it are the core's
 * questions (`workspace_rename` → `wscmd::rename`, which is `charter workspace rename`), and the
 * core's sentence comes back here unchanged. The button asks only that the box holds a name
 * other than the one the workspace already has — which is not a rule about names, it is the
 * difference between a question answered and one that was not.
 */
export function RenameWorkspace({
  workspace,
  /** The core's sentence naming the chats that will start a fresh conversation after the
   *  rename (charter#367, D10), `null` for none, and `undefined` while it is being asked —
   *  the answer waits for it, so it is always read first. The rename still goes ahead. */
  startsFresh,
  /** Why the last attempt renamed nothing — **the core's sentence, unchanged**. */
  trouble,
  /** Whether charter is renaming it right now, so the answer cannot be given twice. */
  renaming,
  onRename,
  onCancel,
}: {
  workspace: string;
  startsFresh?: string | null;
  trouble?: string;
  renaming: boolean;
  onRename: (name: string) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(workspace);
  const nameId = useId();
  const box = useRef<HTMLInputElement>(null);
  const next = name.trim();
  const ready = next !== "" && next !== workspace && !renaming && startsFresh !== undefined;
  const rename = () => {
    if (ready) onRename(next);
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
          className="warning"
          aria-labelledby="rename-workspace"
          // A click outside answers nothing; Escape is Cancel, and nothing is renamed.
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            box.current?.focus();
            box.current?.select();
          }}
        >
          <Dialog.Title id="rename-workspace">Rename workspace {workspace}</Dialog.Title>
          <form
            className="asks"
            onSubmit={(event) => {
              event.preventDefault();
              rename();
            }}
          >
            <label htmlFor={nameId}>New name</label>
            <input
              id={nameId}
              ref={box}
              value={name}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setName(event.target.value)}
            />
            <p className="came-back">
              Its folder under <code>workspaces/</code> moves, its clones&apos; worktrees are
              repaired, and everything that names it follows. Not while a chat is running in it.
            </p>
            {/* The core's sentence, unchanged, as `trouble` is. */}
            {startsFresh && (
              <p className="came-back" aria-label="Chats that will start fresh">
                {startsFresh}
              </p>
            )}
            {/* Verbatim, beside the box: the operator is still answering. */}
            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}

            {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). */}
            <div className="doing">
              <button type="submit" tabIndex={0} disabled={!ready}>
                {renaming ? "Renaming…" : "Rename workspace"}
              </button>
              <button type="button" tabIndex={0} onClick={onCancel}>
                Cancel
              </button>
            </div>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
