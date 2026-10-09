import { createContext, useContext, useRef, type ReactNode } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { AnswerQuestion } from "./AnswerQuestion";
import { useKeyboardBack } from "./Brief";
import type { PlaneId } from "./bindings";

/**
 * **Answer a task's question from where the task is listed** (#1496, #1551): its row in the
 * Chats list (the row's menu) and its line in its tab's menu.
 *
 * Neither can hold the form inline (a row is one button, and a menu is a menu), so both open
 * this one dialog, which holds the same form the Activity tab draws under the question's line
 * ({@link AnswerQuestion}). The form reads the question the task is paused on from the core as
 * it opens, and answers that one by its number.
 *
 * **Offered only while the task is asking** (the record's `asking`, the fact `task_question`
 * answers from). Where the question closes while the dialog is open (the asking chat answered
 * first, or the task moved on), the form says so and keeps what was typed, to copy.
 *
 * **A form, so a press outside it does not close it**: what was typed is not lost to a stray
 * press. Escape, Cancel and Close put it away, and the keyboard goes back to where it was as
 * the dialog opened, as Brief's does ({@link useKeyboardBack}).
 */

/** Which task's question: its chat by the app's number for it, and its name as it is shown. */
export type AnswerAsk = { chat: number; name: string };

/** Opens the Answer dialog for a task: the one way in, for the catalogue's `chat.answer` rows
 *  and a tab menu's line. Nothing is read until it opens. */
export type OpenAnswer = (of: AnswerAsk) => void;

const Opener = createContext<OpenAnswer | undefined>(undefined);

/** Hands everything a project's window draws the way to answer a task's question. */
export function AnswerOpener({ value, children }: { value: OpenAnswer; children: ReactNode }) {
  return <Opener.Provider value={value}>{children}</Opener.Provider>;
}

/** {@link OpenAnswer}, inside a project's window. Outside one there is none, and whatever
 *  would offer Answer offers nothing. */
export function useOpenAnswer(): OpenAnswer | undefined {
  return useContext(Opener);
}

/** The dialog's accessible name, and the name of every control that opens it. */
export function answerTitle(name: string): string {
  return `Answer ${name}'s question`;
}

/** What the form says where the task stopped asking while the dialog was open. */
export function noLongerAsking(name: string): string {
  return `${name} is no longer waiting on this question: it was answered, or the task moved on.`;
}

/**
 * The dialog. Mounted while it is open and unmounted when it closes, so each opening reads the
 * question again. `asking` is whether the task is still paused on a question, as its record
 * says now.
 */
export function AnswerPanel({
  plane,
  of,
  asking,
  onClose,
}: {
  plane: PlaneId;
  of: AnswerAsk;
  asking: boolean;
  onClose: () => void;
}) {
  const handBack = useKeyboardBack();
  const content = useRef<HTMLDivElement>(null);
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content
          ref={content}
          className="warning answer-panel"
          aria-describedby={undefined}
          // **The keyboard is in the dialog from its first frame**, in every state: on the
          // dialog itself while the question is read, or where there is none to answer (then
          // Tab reaches Close and nothing behind it), and in the box once the form has it.
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            content.current?.focus();
          }}
          onPointerDownOutside={(event) => event.preventDefault()}
          onCloseAutoFocus={handBack}
        >
          <Dialog.Title>
            Answer <bdi>{of.name}</bdi>&apos;s question
          </Dialog.Title>
          <AnswerQuestion
            plane={plane}
            session={of.chat}
            task={of.name}
            closed={asking ? undefined : noLongerAsking(of.name)}
            onDone={onClose}
            onCancel={onClose}
          />
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
