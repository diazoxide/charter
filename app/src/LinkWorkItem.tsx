import { useId, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";

/**
 * **Link to work item…**, from a chat tab's menu or the palette (V60, ADR 0088 §3).
 *
 * Asks for the work item's tracker key, such as `github:github.com/owner/repo#12` or
 * `todo:<workspace>/<todo>`, and links the chat to it. A chat works on at most one work item, so
 * linking one that is already linked replaces its link.
 *
 * **It validates nothing**, for `NewBranch`'s reason: what a tracker key is, and which chats
 * may be linked, are the core's rules (`chat_work_link`), so the window refuses exactly what the
 * core refuses and says the same sentence. The Work view (FW-9) will pick the item from a list.
 */
export function LinkWorkItem({
  chat,
  linked,
  trouble,
  linking,
  onLink,
  onCancel,
}: {
  /** The chat's name, as its tab shows it. */
  chat: string;
  /** The work item it works on now, if any. */
  linked?: string;
  /** Why the last attempt linked nothing — **the core's sentence, unchanged**. */
  trouble?: string;
  /** Whether charter is linking it right now, so the answer cannot be given twice. */
  linking: boolean;
  onLink: (key: string) => void;
  onCancel: () => void;
}) {
  const [key, setKey] = useState("");
  const keyId = useId();
  const box = useRef<HTMLInputElement>(null);
  const link = () => {
    if (!linking) onLink(key.trim());
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
          aria-labelledby="link-work-item"
          // A click outside answers nothing, as in every dialog here (`docs/ui-primitives.md`).
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            box.current?.focus();
          }}
        >
          <Dialog.Title id="link-work-item">Link to work item</Dialog.Title>
          <p className="where">
            for <code>{chat}</code>
          </p>

          <form
            className="asks"
            onSubmit={(event) => {
              event.preventDefault();
              link();
            }}
          >
            <label htmlFor={keyId}>Tracker key</label>
            <input
              id={keyId}
              ref={box}
              value={key}
              autoComplete="off"
              spellCheck={false}
              placeholder="github:github.com/owner/repo#12"
              aria-describedby={`${keyId}-why`}
              onChange={(event) => setKey(event.target.value)}
            />
            <p className="came-back" id={`${keyId}-why`}>
              {linked ? (
                <>
                  It works on <code>{linked}</code> now. A chat works on one work item, so this
                  replaces it.
                </>
              ) : (
                <>
                  A chat works on one work item. The link is kept in the workspace&apos;s work link
                  log.
                </>
              )}
            </p>

            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}

            {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). */}
            <div className="doing">
              <button type="submit" tabIndex={0} disabled={linking}>
                Link
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
