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
 * And one that is this dialog's alone: **Cancel is focused, and it is first.** The destructive
 * answer is never the one a stray Return finds, which matters most on the one dialog that
 * appears without being asked for.
 *
 * **Two answers, in that order, and both carrying `tabIndex={0}`.** Radix's `FocusScope`
 * intercepts Tab only at the EDGES of the scope: on the first tabbable it acts on Shift+Tab and
 * moves the focus to the last itself, on the last it acts on Tab and moves to the first, and in
 * between it does nothing and the engine decides. **The engine here is WebKit on both platforms
 * charter ships to, and WebKit leaves a `<button>` out of the tab sequence** unless "tab to all
 * controls" is on — **or the button's `tabindex` is written down**, which is the whole of the
 * fix and is the engine's own rule rather than a workaround
 * (`HTMLFormControlElement::isKeyboardFocusable`; `docs/ui-primitives.md` cites the change).
 *
 * Until charter-app#186 this dialog was whole for a narrower reason: these two are the only
 * tabbables and the confirm is the second, so Cancel IS the first edge and the confirm IS the
 * last, and Shift+Tab from Cancel was Radix's own `focus()` call rather than the engine's tab
 * sequence. That is a property of the *number of buttons*, and it made "the keyboard works
 * here" something a third control could take away in silence. It does not any more. The order
 * still matters and `App.test.tsx` still pins it — Cancel first, so a Return pressed by reflex
 * cancels — but the order is now about which answer a reflex finds and not about reachability.
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
  onEnd,
  onCancel,
}: {
  /** The catalogue row waiting on an answer — `tab.close:<id>` or `pane.close`. Its title is
   *  what the dialog is about, so there is no second wording of what is being ended. */
  offer: Offer;
  onEnd: () => void;
  onCancel: () => void;
}) {
  // Focused by the dialog itself rather than by `autoFocus`: see `StartChat` for why.
  const cancel = useRef<HTMLButtonElement>(null);
  const handBack = useFocusBack();
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
          // **The focus is put on Cancel here although the primitive already does it** —
          // measured: taking this out leaves every test green, because `AlertDialogContent`
          // focuses its `Cancel` itself. It stays because it is the one property a stray
          // Return depends on, and a property that matters is spelled rather than inherited.
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            cancel.current?.focus();
          }}
          onCloseAutoFocus={handBack}
        >
          <Alert.Title>{offer.title}?</Alert.Title>
          {/* The catalogue's own sentence, not a second one written here. It is the same
              string the `×`'s tooltip has carried since charter-app#130. */}
          <Alert.Description className="honest mid-turn">{ENDS_IT}</Alert.Description>
          {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). This
              dialog was already whole, because its two answers ARE the two edges Radix's
              focus scope handles — but that made a property of the keyboard depend on the
              number of buttons, and the attribute is what makes Tab move between them the way
              it does in every other window on the machine. */}
          <div className="answer">
            <Alert.Cancel asChild>
              <button ref={cancel} tabIndex={0}>
                Cancel
              </button>
            </Alert.Cancel>
            <Alert.Action asChild>
              <button className="ends-it" tabIndex={0} onClick={onEnd}>
                {offer.title}
              </button>
            </Alert.Action>
          </div>
        </Alert.Content>
      </Alert.Portal>
    </Alert.Root>
  );
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
