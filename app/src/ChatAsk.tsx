import * as AlertDialog from "@radix-ui/react-alert-dialog";

/**
 * **The question a chat Notice asks before it does what cannot be taken back** (NO-3): Forget
 * this chat… (its record is dropped) and Start fresh (its program ends, and it starts again).
 *
 * Radix's `AlertDialog`, for `EndingChat`'s reasons: an answer that loses something is a
 * question that must be answered, Escape is Cancel, and a click outside answers nothing. Cancel
 * is first and has the focus, so a stray Return loses nothing. **A refusal stays in the
 * question** (`trouble`), in the core's words, so the operator reads why next to what they
 * asked, and the thing it was about is left as it was.
 */
export function ChatAsk({
  title,
  says,
  answer,
  trouble,
  busy,
  onAnswer,
  onCancel,
}: {
  title: string;
  /** What happens, said plainly. */
  says: string;
  /** The button that does it: its verb. */
  answer: string;
  /** The core's refusal of the last answer. */
  trouble?: string;
  /** Whether the answer is being carried out, so it cannot be given twice. */
  busy: boolean;
  onAnswer: () => void;
  onCancel: () => void;
}) {
  return (
    <AlertDialog.Root
      open
      onOpenChange={(open) => {
        if (!open && !busy) onCancel();
      }}
    >
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="asking" />
        <AlertDialog.Content className="warning">
          <AlertDialog.Title>{title}</AlertDialog.Title>
          <AlertDialog.Description className="honest">{says}</AlertDialog.Description>
          {trouble && (
            <p className="trouble" role="alert">
              {trouble}
            </p>
          )}
          <div className="answer">
            {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). */}
            <AlertDialog.Cancel asChild>
              <button type="button" tabIndex={0} disabled={busy}>
                Cancel
              </button>
            </AlertDialog.Cancel>
            <button
              type="button"
              className="ends-it"
              tabIndex={0}
              disabled={busy}
              onClick={onAnswer}
            >
              {answer}
            </button>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
