import { useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useFocusBack } from "./EndingChat";
import { Field, SettingActions, SettingRow } from "./settings/components";

/**
 * **Link to work item…**, from a chat tab's menu or the palette (V60, ADR 0088 §3).
 *
 * Asks for the work item's tracker key, such as `github:github.com/owner/repo#12`,
 * `gitlab:gitlab.com/group/repo#12` or `todo:<workspace>/<todo>`, and links the chat to it. A chat works on one work item at most, so
 * a new link replaces the one it has.
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
  // The dialog, so the box can be found in it on opening: the row draws the box, and a
  // setting's field takes no ref.
  const content = useRef<HTMLDivElement>(null);
  const handBack = useFocusBack();
  // Sent as typed, spaces and all: a key is refused, never rewritten (D-0021), so the core's
  // sentence says what is wrong with it.
  const link = () => {
    if (!linking) onLink(key);
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
          // A click outside answers nothing, as in every dialog here (`docs/ui-primitives.md`).
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            content.current?.querySelector("input")?.focus();
          }}
          // The focus goes back to where it was as the dialog opened, as `EndingChat`'s does.
          onCloseAutoFocus={handBack}
        >
          <Dialog.Title>Link to work item</Dialog.Title>
          {/* The dialog's description (#1719): where the link goes, said with the title. */}
          <Dialog.Description className="where">
            For <code>{chat}</code>
          </Dialog.Description>

          <form
            onSubmit={(event) => {
              event.preventDefault();
              link();
            }}
          >
            <SettingRow
              label="Tracker key"
              help={
                <>
                  On GitHub, a key looks like <code>github:github.com/owner/repo#12</code>; on
                  GitLab, like <code>gitlab:gitlab.com/group/repo#12</code>.{" "}
                  {linked ? (
                    <>
                      This chat works on <code>{linked}</code>. Linking another item replaces that
                      link.
                    </>
                  ) : (
                    <>
                      A chat works on one work item at a time. The link is kept with the workspace,
                      so your other devices see it when the workspace is LIVE.
                    </>
                  )}
                </>
              }
              control={(ids) => (
                <Field
                  ids={ids}
                  kind="text"
                  value={key}
                  placeholder="github:github.com/owner/repo#12"
                  onChange={setKey}
                />
              )}
            />

            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}

            {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). */}
            <SettingActions>
              {/* A verb and what it acts on, and what it is doing while it does it (#630). */}
              <button type="submit" tabIndex={0} disabled={linking}>
                {linking ? "Linking…" : "Link work item"}
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
