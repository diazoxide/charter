import { createContext, useContext, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useFocusBack } from "./EndingChat";
import { Field, SettingActions, SettingRow } from "./settings/components";

/**
 * **Ask a persona…**, from a chat tab's menu, the palette, or a Notice that names the persona
 * to ask.
 *
 * You ask a persona for something from the chat you are in: a chat starts as that persona,
 * under this one, on what you typed. It runs with that persona's own sandbox, hosts and
 * vaults, and nothing this chat holds. Its report comes back to this chat, marked as started
 * by you. Nothing is asked of you first, because you are the one asking.
 *
 * **It validates nothing but that there is something to send**, for `NewBranch`'s reason: what
 * a task may be called, whether the persona can run a chat and how many may run at once are
 * the core's rules (`ask_persona_chat`), so the window refuses what the core refuses and says
 * the same sentence.
 */
export function AskPersona({
  persona,
  chat,
  prefill,
  trouble,
  asking,
  onAsk,
  onCancel,
}: {
  /** The persona to ask. */
  persona: string;
  /** The chat it is asked from, as its tab shows it: where the report comes back. */
  chat: string;
  /** What the boxes start with, where whoever opened the dialog already knows. */
  prefill?: AskPrefill;
  /** Why the last attempt started nothing, in the core's sentence, unchanged. */
  trouble?: string;
  /** Whether purlis is starting the chat right now, so the answer cannot be given twice. */
  asking: boolean;
  onAsk: (name: string, ask: string) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(prefill?.name ?? "");
  const [ask, setAsk] = useState(prefill?.ask ?? "");
  // The dialog, so the first box can be found in it on opening: the row draws the box, and a
  // setting's field takes no ref.
  const content = useRef<HTMLDivElement>(null);
  const handBack = useFocusBack();
  const ready = name.trim() !== "" && ask.trim() !== "" && !asking;
  const send = () => {
    if (ready) onAsk(name, ask);
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
            // A prefilled name leaves the question as the first thing to write.
            const boxes = content.current?.querySelectorAll<HTMLElement>("input, textarea");
            const first = prefill?.name ? boxes?.[1] : boxes?.[0];
            (first ?? boxes?.[0])?.focus();
          }}
          // The focus goes back to where it was as the dialog opened, as `EndingChat`'s does.
          onCloseAutoFocus={handBack}
        >
          <Dialog.Title>Ask {persona}</Dialog.Title>
          <p className="where">
            from <code>{chat}</code>
          </p>

          <form
            onSubmit={(event) => {
              event.preventDefault();
              send();
            }}
          >
            <SettingRow
              label="Task name"
              help="What the new chat is called, on its tab and under this chat."
              control={(ids) => (
                <Field
                  ids={ids}
                  kind="text"
                  value={name}
                  placeholder="check the queue"
                  onChange={setName}
                />
              )}
            />
            <SettingRow
              label="What to ask"
              help={
                <>
                  A chat starts as {persona} on these words, with {persona}&apos;s own sandbox,
                  hosts and vaults. Its report comes back to <code>{chat}</code>, marked as started
                  by you.
                </>
              }
              control={(ids) => (
                <Field ids={ids} kind="list" value={ask} minRows={4} onChange={setAsk} />
              )}
            />

            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}

            {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). */}
            <SettingActions>
              <button type="submit" tabIndex={0} disabled={!ready}>
                {asking ? `Asking ${persona}…` : `Ask ${persona}`}
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

/** What the dialog's boxes start with. */
export type AskPrefill = { name?: string; ask?: string };

/**
 * Opens **Ask {persona}** for chat `session`, with its boxes prefilled where the caller knows
 * what to ask: the one way into the dialog for anything that is not a row of the catalogue.
 *
 * A Notice on a chat's pane that names the persona to go to (a vault this chat's persona was
 * refused, which is tagged for another) calls this with that persona, so its button opens the
 * same dialog the tab's menu does and starts the same chat. Nothing starts until the dialog is
 * answered.
 */
export type OpenAskPersona = (session: number, persona: string, prefill?: AskPrefill) => void;

const Opener = createContext<OpenAskPersona>(() => undefined);

/** Hands every pane of a project the way to open **Ask {persona}** ({@link OpenAskPersona}). */
export const AskPersonaOpener = Opener.Provider;

/** {@link OpenAskPersona}, for a Notice inside a project's panes. Outside one it opens nothing. */
export function useAskPersona(): OpenAskPersona {
  return useContext(Opener);
}
