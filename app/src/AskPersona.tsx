import { createContext, useContext, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useFocusBack } from "./EndingChat";
import { Choice, Field, SettingActions, SettingRow } from "./settings/components";

/** Where the new chat works, as the dialog holds the pick. */
type Where = "here" | "branch" | "workspace";

/**
 * **The word the core reads for where a chat works** (`ask_persona_chat`'s `place`, #1453): none
 * for the asking chat's folder, `worktree` for a branch of its own, `workspace:<name>` for
 * another workspace. The same two words a chat's own dispatch has, and nothing else: the dialog
 * names no folder and no branch.
 */
export function placeOf(where: Where, workspace: string): string | null {
  if (where === "branch") return "worktree";
  if (where === "workspace" && workspace !== "") return `workspace:${workspace}`;
  return null;
}

/**
 * **Ask a persona…**, from a chat tab's menu, the palette, or a Notice that names the persona
 * to ask.
 *
 * You ask a persona for something from the chat you are in: a chat starts as that persona,
 * under this one, on what you typed. It runs with that persona's own sandbox, hosts and
 * vaults, and nothing this chat holds. Its report comes back to this chat, marked as started
 * by you. Nothing is asked of you first, because you are the one asking.
 *
 * **Where it works is one of three places** (#1453): this chat's folder, a branch of its own
 * that purlis cuts from the repo this chat works in (the window says *branch*, ADR 0072 §4),
 * or another workspace of the project. Nothing is merged for a branch of its own.
 *
 * **It validates nothing but that there is something to send**, for `NewBranch`'s reason: what
 * a task may be called, whether the persona can run a chat, how many may run at once and
 * whether this chat works in a repo a branch can be cut from are the core's rules
 * (`ask_persona_chat`), so the window refuses what the core refuses and says the same sentence.
 */
export function AskPersona({
  persona,
  chat,
  workspaces,
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
  /** The project's other workspaces, by name: where else the new chat can work. */
  workspaces: readonly string[];
  /** Whether purlis is starting the chat right now, so the answer cannot be given twice. */
  asking: boolean;
  /** `place` is the core's word for where it works ({@link placeOf}); `null` is this chat's
   *  folder. */
  onAsk: (name: string, ask: string, place: string | null) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(prefill?.name ?? "");
  const [ask, setAsk] = useState(prefill?.ask ?? "");
  const [where, setWhere] = useState<Where>("here");
  const [workspace, setWorkspace] = useState("");
  // The dialog, so the first box can be found in it on opening: the row draws the box, and a
  // setting's field takes no ref.
  const content = useRef<HTMLDivElement>(null);
  const handBack = useFocusBack();
  // Another workspace is a pick that needs its second half before there is something to send.
  const placed = where !== "workspace" || workspace !== "";
  const ready = name.trim() !== "" && ask.trim() !== "" && placed && !asking;
  const send = () => {
    if (ready) onAsk(name, ask, placeOf(where, workspace));
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

            <SettingRow
              label="Where it works"
              control={(ids) => (
                <Choice
                  ids={ids}
                  kind="radio"
                  value={where}
                  onValueChange={(to) => setWhere(to as Where)}
                  options={[
                    {
                      value: "here",
                      label: "This chat's folder",
                      says: (
                        <>
                          Beside <code>{chat}</code>, in the same files.
                        </>
                      ),
                    },
                    {
                      value: "branch",
                      label: "A branch of its own",
                      says: "purlis cuts a new branch, in a folder of its own, from the repo this chat works in. Nothing is merged for it: its report names the branch.",
                    },
                    {
                      value: "workspace",
                      label: "Another workspace",
                      says: "In that workspace's folder, with its todos, memory and session records.",
                      disabled: workspaces.length === 0,
                    },
                  ]}
                />
              )}
            />
            {where === "workspace" && (
              <SettingRow
                label="Workspace"
                control={(ids) => (
                  <Choice
                    ids={ids}
                    kind="select"
                    value={workspace}
                    unset="Choose a workspace"
                    onValueChange={setWorkspace}
                    options={workspaces.map((one) => ({ value: one, label: one }))}
                  />
                )}
              />
            )}

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
