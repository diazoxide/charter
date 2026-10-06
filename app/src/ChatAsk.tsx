import * as AlertDialog from "@radix-ui/react-alert-dialog";
import type { State } from "./chatState";

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
  warns,
  trouble,
  busy,
  onAnswer,
  onCancel,
  onCloseAutoFocus,
}: {
  title: string;
  /** What happens, said plainly. */
  says: string;
  /** The button that does it: its verb. */
  answer: string;
  /** What the answer would interrupt, said beside what it does ({@link midTurnWarning}). */
  warns?: string;
  /** The core's refusal of the last answer. */
  trouble?: string;
  /** Whether the answer is being carried out, so it cannot be given twice. */
  busy: boolean;
  onAnswer: () => void;
  onCancel: () => void;
  /** Where the focus goes as the question closes; Radix's own return when not given. */
  onCloseAutoFocus?: (event: Event) => void;
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
        <AlertDialog.Content className="warning" onCloseAutoFocus={onCloseAutoFocus}>
          <AlertDialog.Title>{title}</AlertDialog.Title>
          <AlertDialog.Description className="honest">{says}</AlertDialog.Description>
          {warns && <p className="honest mid-turn">{warns}</p>}
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

/**
 * **What ending a chat's program would interrupt** (#1246), as the quit warning says it
 * (`MidTurnSaid`): a chat the board says is mid-turn, and one whose harness reports nothing, so
 * charter cannot tell. Nothing for a chat that is waiting, done or failed, nor for a shell
 * nothing has reported on (`state` undefined).
 */
export function midTurnWarning(name: string, state: State | undefined): string | undefined {
  if (state === "running") return `${name} is mid-turn, and the turn will be interrupted.`;
  if (state === "unknown")
    return `${name} reports no state, so charter cannot tell whether it is mid-turn.`;
  return undefined;
}

/**
 * **Where the keyboard goes once a question has made its own Notice go** (#1246): Forget this
 * chat… drops the Notice it was asked from, so Radix's return to it would land on the page.
 * The next Notice standing in the same band, at its first button; with none, the strip's tab in
 * front.
 *
 * `band` is the band the Notice stood in, found when the question was asked: by then the Notice
 * is gone, and WebKit does not focus a pressed button, so the focus cannot say where it was.
 */
export function focusAfterNoticeGone(band: Element | null, strip: Element | null): void {
  const next =
    band?.querySelector<HTMLElement>(".notice button") ??
    strip?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]') ??
    strip?.querySelector<HTMLElement>('[role="tab"]');
  next?.focus();
}
