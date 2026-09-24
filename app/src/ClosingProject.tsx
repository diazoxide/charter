import { useRef } from "react";
import * as Alert from "@radix-ui/react-alert-dialog";
import { useFocusBack } from "./EndingChat";
import { ChatState } from "./NeedsYou";
import type { Ending } from "./QuitWarning";

/**
 * What charter asks before it closes a project that has chats open.
 *
 * **The project's `×` ended every chat in it without a word**, and that was tolerable while it
 * was a small target under the pointer. charter-app#239 put the same row one key away — Delete
 * on the focused tab, and on a Mac the ordinary delete key — and the operator ruled that
 * closing a project with chats running asks first, as ending one chat does (`EndingChat`).
 *
 * **Asked where the row's verb is carried out**, `closeProject` in `App.tsx`, so every surface
 * that presses `project.close:<plane>` asks it: the `×`, Delete, the tab's menu and the
 * palette. A project with nothing open closes as it always did, without a question.
 *
 * **It says what goes, in the quit warning's words and rows** (`QuitWarning.tsx`): how many
 * chats end, in which workspaces, and each chat with what it is doing. A question that said
 * only "are you sure?" would be asking the operator to remember what is in a project they
 * may not be looking at.
 *
 * **An `AlertDialog`, for `EndingChat`'s reasons**: it arrives because of something the
 * operator did, a click outside answers nothing, Escape answers Cancel, and Cancel is first and
 * focused so a stray Return keeps everything. When it closes, the focus goes back to where it
 * was (`useFocusBack`) — the project's tab, when the question came from Delete on it.
 */
export function ClosingProject({
  name,
  chats,
  heard,
  onClose,
  onCancel,
}: {
  /** The project's name, as its tab says it. */
  name: string;
  /** Every chat it has open, which is what closing it ends. */
  chats: readonly Ending[];
  /** Whether the project has said what it has open. Until it has, the list may be short. */
  heard: boolean;
  onClose: () => void;
  onCancel: () => void;
}) {
  const cancel = useRef<HTMLButtonElement>(null);
  const handBack = useFocusBack();
  const count = chats.length === 1 ? "1 chat" : `${chats.length} chats`;
  // The workspaces in the order their chats are listed, each once.
  const where = [...new Set(chats.map((chat) => chat.workspace).filter(Boolean))];
  const title = chats.length > 0 ? `Close project ${name} and end ${count}?` : `Close ${name}?`;
  return (
    <Alert.Root
      open
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <Alert.Portal>
        <Alert.Overlay className="asking" />
        <Alert.Content
          className="warning"
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            cancel.current?.focus();
          }}
          onCloseAutoFocus={handBack}
        >
          <Alert.Title>{title}</Alert.Title>
          <Alert.Description className="honest mid-turn">
            {chats.length > 0 && `${count} ${chats.length === 1 ? "ends" : "end"}`}
            {where.length > 0 && ` in ${where.join(", ")}`}
            {chats.length > 0 && ". Ending a chat ends the program it runs. There is no undo. "}
            {!heard && "charter has not yet heard what this project has open, so it may be more. "}
            Nothing of the project on disk goes.
          </Alert.Description>
          {chats.length > 0 && (
            <ul className="ending">
              {chats.map((chat) => (
                <li key={chat.key}>
                  <span className="what">{chat.harness ?? "shell"}</span>
                  <span className="who">{chat.name}</span>
                  <ChatState state={chat.state} />
                  {chat.cwd && <code className="where">{chat.cwd}</code>}
                </li>
              ))}
            </ul>
          )}
          {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). */}
          <div className="answer">
            <Alert.Cancel asChild>
              <button ref={cancel} tabIndex={0}>
                Cancel
              </button>
            </Alert.Cancel>
            <Alert.Action asChild>
              <button className="ends-it" tabIndex={0} onClick={onClose}>
                {chats.length > 0 ? `Close and end ${count}` : "Close"}
              </button>
            </Alert.Action>
          </div>
        </Alert.Content>
      </Alert.Portal>
    </Alert.Root>
  );
}
