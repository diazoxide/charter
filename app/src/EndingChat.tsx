import { useRef } from "react";
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
 * **Exactly two answers, in that order, and it is load-bearing rather than tidy.** Radix's
 * `FocusScope` intercepts Tab only at the EDGES of the scope: on the first tabbable it acts on
 * Shift+Tab and moves the focus to the last itself, on the last it acts on Tab and moves to the
 * first, and in between it does nothing and the engine decides. **The engine here is WebKit on
 * both platforms charter's scenarios run on, and WebKit does not put a `<button>` in the tab
 * sequence at all** — measured in charter-app#176, where a scenario pressing plain Tab left this
 * question on screen on `webkit macos` and again on `WebKitGTK linux`.
 *
 * So the confirm is reachable by keyboard *because* these two are the only tabbables and the
 * confirm is the second: Cancel IS the first edge and the confirm IS the last, and Shift+Tab
 * from Cancel is Radix's own `focus()` call rather than the engine's tab sequence. **A third
 * focusable between them would be unreachable by keyboard on both platforms** — the engine will
 * not tab to it and Radix only handles the edges. `App.test.tsx` fails on a third rather than
 * leaving it to a scenario run to find.
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
        >
          <Alert.Title>{offer.title}?</Alert.Title>
          {/* The catalogue's own sentence, not a second one written here. It is the same
              string the `×`'s tooltip has carried since charter-app#130. */}
          <Alert.Description className="honest mid-turn">{ENDS_IT}</Alert.Description>
          <div className="answer">
            <Alert.Cancel asChild>
              <button ref={cancel}>Cancel</button>
            </Alert.Cancel>
            <Alert.Action asChild>
              <button className="ends-it" onClick={onEnd}>
                {offer.title}
              </button>
            </Alert.Action>
          </div>
        </Alert.Content>
      </Alert.Portal>
    </Alert.Root>
  );
}
