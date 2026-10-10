import { useRef } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { ChatsToPick, referenceSaid, type ChatHere, type Referenced } from "./references";

/**
 * **"Add to a chat's context" from a file or folder row** (FM-9, #1151): the project's chats,
 * and the one picked has a reference to `referenced` typed in, never sent.
 *
 * The same picker the preview's *Add to a chat's context* and a search hit's Shift+Enter draw
 * (`ChatsToPick`), in a dialog because a row's menu has closed by the time it runs. It types
 * nothing itself: `onPick` hands the reference to the window's one way into a chat
 * (`ChatsForReferences.hand`), whose answer is said where every row's answer is.
 */
export function AddToAChat({
  referenced,
  chats,
  onPick,
  onCancel,
}: {
  referenced: Referenced;
  chats: readonly ChatHere[];
  onPick: (chat: ChatHere) => void;
  onCancel: () => void;
}) {
  const list = useRef<HTMLDivElement>(null);
  const label = `Add ${referenceSaid(referenced)} to a chat's context`;
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
          aria-describedby={undefined}
          onOpenAutoFocus={(event) => {
            // The keyboard starts on the first chat, as the vault picker's starts on its first row.
            const first = list.current?.querySelector<HTMLElement>("button");
            if (first) {
              event.preventDefault();
              first.focus();
            }
          }}
        >
          <Dialog.Title>{label}</Dialog.Title>
          <div ref={list}>
            <ChatsToPick
              chats={chats}
              label="Chats"
              onPick={(chat) => {
                onCancel();
                onPick(chat);
              }}
            />
          </div>
          <div className="doing">
            <button type="button" tabIndex={0} onClick={onCancel}>
              Cancel
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
