import { useRef, useState } from "react";
import * as Alert from "@radix-ui/react-alert-dialog";
import { ENDS_IT, type Offer } from "./actions";

/**
 * What charter asks before it ends a chat.
 *
 * **The operator asked for it in the same breath as the pane controls**: *"closing session
 * should ask confirmation"*. Until now the only guard was the words — `End chat 3 steward` on
 * the `×`, and a tooltip saying *"Ends the program it runs. There is no undo."* That was the
 * fix for charter-app#130, where a glyph reading "hide this tab" was ending live harnesses,
 * and it was the right fix for a *name*. It is not a guard against the press itself, and the
 * press is now one of several: a `×` on a tab, a `×` on a pane that appears under the pointer
 * on hover, and a row of the palette.
 *
 * **Every route through it, because there is one list of actions.** This is asked in
 * `PlaneView`'s `run`, which is what carries out a row from whichever surface pressed it — so
 * the palette's `End chat 3 steward` asks exactly as the tab's `×` does. A confirmation on one
 * surface and not another is the second answer this app's catalogue exists to not have.
 *
 * **Radix's `AlertDialog` and not its `Dialog`**, which is not a style choice: an alert dialog
 * is `role="alertdialog"`, it is announced as an interruption rather than as a surface, and
 * the primitive requires a `Cancel` that the focus goes to. The four dialogs in
 * `docs/ui-primitives.md` are questions the operator went looking for; this one arrives
 * *because of* something they did, which is the distinction the role exists for.
 *
 * The two house rules for a modal here are the four dialogs' (`docs/ui-primitives.md`), and
 * this primitive keeps both without being told:
 *
 * - **A click outside answers nothing.** The four `Dialog`s prevent `onInteractOutside` by
 *   hand; `AlertDialogContent` does not take the prop at all, because it refuses outside
 *   interaction itself.
 * - **Escape answers, with the non-destructive answer** — the primitive's own `Cancel`.
 *
 * And one that is this dialog's alone: **Cancel is first, and the focus starts on the answer the
 * operator ruled the default** (ADR 0064). Since Smart close there are three answers — Cancel,
 * Close, Smart close — and the focus goes to Smart close on a chat with turns behind it, to
 * Close on one that has had at most one turn (little to record), and to Cancel wherever
 * charter cannot say (`firstAnswer`). Before Smart close, Cancel was always focused; a stray
 * Return on a chat charter knows nothing about still cancels.
 *
 * **Every answer carries `tabIndex={0}`.** Radix's `FocusScope`
 * intercepts Tab only at the EDGES of the scope: on the first tabbable it acts on Shift+Tab and
 * moves the focus to the last itself, on the last it acts on Tab and moves to the first, and in
 * between it does nothing and the engine decides. **The engine here is WebKit on both platforms
 * charter ships to, and WebKit leaves a `<button>` out of the tab sequence** unless "tab to all
 * controls" is on — **or the button's `tabindex` is written down**, which is the whole of the
 * fix and is the engine's own rule rather than a workaround
 * (`HTMLFormControlElement::isKeyboardFocusable`; `docs/ui-primitives.md` cites the change).
 *
 * Until charter-app#186 this dialog was whole for a narrower reason: it had two tabbables and
 * the confirm was the second, so Cancel WAS the first edge and the confirm the last, and
 * Shift+Tab from Cancel was Radix's own `focus()` call rather than the engine's tab sequence.
 * That was a property of the *number of buttons*, and the third one Smart close added is why it
 * had to stop being one. `App.test.tsx` and `SmartClose.test.tsx` pin the order and the focus.
 *
 * **What the above is NOT is the reason a scenario cannot press these buttons**, and an earlier
 * version of this comment said it was. Measured in charter-app#176 with a keydown trace in the
 * real WebView: `Enter` on a focused `Cancel` arrives AT that button, unprevented, and does not
 * activate it — WebDriver key actions carry no implicit activation. That is the harness, not the
 * engine and not this dialog, and it is why the keyboard half of the claim is tested in
 * `App.test.tsx` and not in `palette.e2e.ts`. Two findings, one true of the product and one true
 * only of the test rig; keeping them apart is the whole point of writing them down.
 */
export function EndingChat({
  offer,
  smart,
  onEnd,
  onSmartClose,
  onCancel,
}: {
  /** The catalogue row waiting on an answer — `tab.close:<id>` or `pane.close`. Its title is
   *  what the dialog is about, so there is no second wording of what is being ended. */
  offer: Offer;
  /** Whether the chat is offered **Smart close** (ADR 0064), as the core answered — or none,
   *  when charter could not say, which offers Close only. */
  smart?: SmartAsk;
  onEnd: () => void;
  onSmartClose: () => void;
  onCancel: () => void;
}) {
  // Focused by the dialog itself rather than by `autoFocus`: see `StartChat` for why.
  const cancel = useRef<HTMLButtonElement>(null);
  const close = useRef<HTMLButtonElement>(null);
  const smartClose = useRef<HTMLButtonElement>(null);
  const handBack = useFocusBack();
  const first = firstAnswer(smart);
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
          // **No `onInteractOutside` here, and that is the primitive rather than an
          // omission.** `AlertDialogContent` does not take one: it prevents outside
          // interaction itself, because an alert dialog is a question that must be answered.
          // The four `Dialog`s in `docs/ui-primitives.md` write the same rule out by hand
          // because `Dialog` would otherwise close on a click outside; this one cannot.
          // **The focus is put on the default answer here** (`firstAnswer`): Smart close for a
          // chat with turns behind it, Close for one with at most one (the operator's ruling,
          // ADR 0064), and Cancel wherever charter cannot say — so a stray Return never ends a
          // chat charter knows nothing about.
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            const focus = { cancel, close, smart: smartClose }[first];
            focus.current?.focus();
          }}
          onCloseAutoFocus={handBack}
        >
          <Alert.Title>{offer.title}?</Alert.Title>
          {/* The catalogue's own sentence, not a second one written here. It is the same
              string the `×`'s tooltip has carried since charter-app#130. */}
          <Alert.Description className="honest mid-turn">
            {ENDS_IT} {smart?.available ? SMART_CLOSE_SAYS : null}
          </Alert.Description>
          {smart?.why && (
            <p className="honest" id="smart-close-why">
              {smart.why}
            </p>
          )}
          {/* `tabIndex={0}` on each, per `docs/ui-primitives.md` (charter-app#186). Cancel
              first and Smart close last, at the edge where a primary answer sits. */}
          <div className="answer">
            <Alert.Cancel asChild>
              <button ref={cancel} tabIndex={0}>
                Cancel
              </button>
            </Alert.Cancel>
            <Alert.Action asChild>
              <button ref={close} className="ends-it" tabIndex={0} onClick={onEnd}>
                Close
              </button>
            </Alert.Action>
            <Alert.Action asChild>
              <button
                ref={smartClose}
                className="smart-close"
                tabIndex={0}
                disabled={!smart?.available}
                aria-describedby={smart?.why ? "smart-close-why" : undefined}
                onClick={onSmartClose}
              >
                Smart close
              </button>
            </Alert.Action>
          </div>
        </Alert.Content>
      </Alert.Portal>
    </Alert.Root>
  );
}

/** What **Smart close** does, said beside Close's cost when it is offered. */
export const SMART_CLOSE_SAYS =
  "Smart close first asks the chat to write its session record, and closes it once the record is saved.";

/** Whether a chat is offered Smart close, as the dialog draws it: the core's answer
 *  (`smart_close_offer`), or the window's own reason where the close is about more than one chat. */
export type SmartAsk = {
  available: boolean;
  why: string | null;
  /** Close is the default: the chat has had at most one turn. */
  close_first: boolean;
};

/** The answer the dialog's focus starts on (ADR 0064). */
export function firstAnswer(smart: SmartAsk | undefined): "smart" | "close" | "cancel" {
  if (smart === undefined) return "cancel";
  if (smart.close_first) return "close";
  return smart.available ? "smart" : "cancel";
}

/**
 * **Where the focus was when a question arrived, handed back when it goes** — for an
 * `AlertDialog` that has no `Trigger`, as a dialog the window raises on the operator's behalf
 * does not.
 *
 * Radix returns the focus to the dialog's trigger, and with none it returns it nowhere: the
 * page. That was invisible while these questions came from a click on a `×`. Since
 * charter-app#239 they also come from Delete on a focused tab, and a Cancel that dropped the
 * keyboard on the page would leave the operator nowhere with nothing ended. So the element
 * that had the focus as the dialog opened gets it back — when it is still there. When it is not
 * (the tab it was on has just closed), the focus is left for whoever put it there to place:
 * `tabKeys.ts` puts it back on the strip.
 */
export function useFocusBack() {
  const [had] = useState(() => document.activeElement);
  return (event: Event) => {
    event.preventDefault();
    if (had instanceof HTMLElement && had.isConnected) had.focus();
  };
}
