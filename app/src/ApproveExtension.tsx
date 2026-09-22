import { useRef } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import type { ExtensionAsk } from "./bindings";

/**
 * What this extension says it brings, what charter will actually do about it, and — in charter's
 * own plain voice — what charter is not able to stop it doing.
 *
 * **The last part is the reason this component exists and is why it is written the way it is.**
 * The operator ruled on 2026-09-22 that charter ships the extension runtime with no OS sandbox,
 * with marketplace vetting and untrusted-source warnings as the deferred answer. charter ADR
 * 0041's honesty paragraph is what he ruled against: *a subprocess does not confine an extension
 * below the operator. It runs as the same user, with the same filesystem, the same network and
 * the same ability to `exec`.* So the capability list is a statement about **charter's conduct**
 * and not a cage, and a dialog that listed "this extension may: contribute a theme" as though
 * that were the limit would manufacture confidence charter cannot back. **That is worse than no
 * dialog**, because a surface that over-promises is one the operator stops reading and then
 * trusts anyway.
 *
 * So the two sentences that say so are `ask.runs_as_you` and `ask.fingerprint_note`, they come
 * from `charter_core::extension`, they are pinned by tests in that crate, and this component
 * renders them **as given**. Nothing here composes a reassurance of its own. Nothing here
 * summarises them shorter.
 *
 * The rest follows ADR 0035's first-open prompt, which is the pattern: show what it contributes,
 * ask once per machine, remember the fingerprint, re-ask when what it contributes changes. The
 * one difference, and it is 0041's: **0035 fingerprints configuration and this fingerprints
 * code.** An extension's path is not its contents, so the hash is over the manifest and every
 * file it declares, re-taken at each launch.
 *
 * A Radix dialog (`docs/ui-primitives.md`), so "has to be answered" is a property of the surface
 * rather than a claim about how it was drawn. Escape answers it the way Cancel does: nothing is
 * approved and the next ask is a first ask again. A click outside is not an answer — missing a
 * dialog is not a decision.
 */
export function ApproveExtension({
  ask,
  onApprove,
  onCancel,
}: {
  ask: ExtensionAsk;
  /** The operator's yes, carrying back the question they were shown — so the approval is for
   *  the bytes on screen and not for whatever is in the directory by the time it is clicked. */
  onApprove: (ask: ExtensionAsk) => void;
  onCancel: () => void;
}) {
  // Cancel, focused by the dialog itself rather than by tab order. Approving an extension puts
  // code on this machine into charter's own trust record, and it is never what a stray Return
  // key finds.
  const cancel = useRef<HTMLButtonElement>(null);
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
          aria-labelledby="approve-extension"
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            cancel.current?.focus();
          }}
        >
          <Dialog.Title id="approve-extension">
            {ask.first
              ? `Trust the extension “${ask.name}”?`
              : `“${ask.name}” has changed since you approved it`}
          </Dialog.Title>
          <p className="where">
            <code>{ask.path}</code>
          </p>

          <h3>{ask.first ? "What it declares" : "What it declares now"}</h3>
          <ul className="contributes">
            {ask.declares.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>

          {/* charter's own words, rendered as given. See this module's header for why nothing
            here rewrites, shortens or softens them. */}
          <p className="came-back runs-as-you">{ask.runs_as_you}</p>
          <p className="came-back">{ask.fingerprint_note}</p>

          <div className="doing">
            <button type="button" onClick={() => onApprove(ask)}>
              {ask.first ? "Trust it" : "Trust it anyway"}
            </button>
            <button type="button" ref={cancel} onClick={onCancel}>
              Cancel
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
