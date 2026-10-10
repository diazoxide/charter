import { useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { commands } from "./bindings";
import { Choice, Field, SettingActions, SettingRow } from "./settings/components";

/**
 * Making a persona (SI-3), asked where the answer is given — `NewVault`'s shape, for `charter
 * persona create <name> [--role …] [--delegate-when …] [--extends …]`.
 *
 * **This dialog validates nothing** but that the question has been answered: a name, and a
 * routing line unless the persona inherits one. What a persona may be called, whether one is
 * already defined by that name, and which values would break its frontmatter are all
 * `personaverbs::define::create`'s to say, through `persona_create`, so the window and a
 * terminal refuse the same things in the same words. An empty box is a flag not given, so the
 * core's defaults apply: the role is the name, title-cased, and the vault is the persona's name.
 *
 * **What it makes is a draft.** The scaffold holds only true statements, and `draft: true` keeps
 * the persona from being dispatched until its charter is written — in the operator's editor,
 * which is where Edit persona.md hands it. charter draws no editor for prose.
 */
export function NewPersona({
  plane,
  trouble,
  making,
  onCreate,
  onCancel,
}: {
  /** Where the persona is written, so the dialog says so. */
  plane: string;
  /** Why the last attempt made nothing — the core's sentence, unchanged. */
  trouble?: string;
  /** Whether charter is making it right now, so the answer cannot be given twice. */
  making: boolean;
  onCreate: (
    name: string,
    role: string | null,
    delegateWhen: string | null,
    parent: string | null,
  ) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [role, setRole] = useState("");
  const [when, setWhen] = useState("");
  const [parent, setParent] = useState("");
  // **The project's personas, read here** (#1719): Inherits from is a pick of them, so a
  // typo is refused before the core sees it. Read from the sidebar's answer, which holds the
  // list; a failure is said, and None still makes a persona.
  const [offered, setOffered] = useState<{ names: string[] } | { unread: string }>();
  useEffect(() => {
    let gone = false;
    void commands
      .planeSidebar(plane)
      .then((answer) => {
        if (gone) return;
        if (answer.status === "error") setOffered({ unread: answer.error });
        else if (Array.isArray(answer.data?.personas)) setOffered({ names: answer.data.personas });
      })
      .catch((err: unknown) => {
        if (!gone) setOffered({ unread: String(err) });
      });
    return () => {
      gone = true;
    };
  }, [plane]);
  // The dialog, so the name box can be found in it on opening: the row draws the box, and a
  // setting's field takes no ref.
  const content = useRef<HTMLDivElement>(null);
  const given = (text: string) => (text.trim() === "" ? null : text.trim());
  const ready = name.trim() !== "" && (given(when) !== null || given(parent) !== null) && !making;
  const create = () => {
    if (ready) onCreate(name.trim(), given(role), given(when), given(parent));
  };
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        // Not while charter is making it, `NewVault`'s reason: a refusal would land where
        // nobody is looking.
        if (!open && !making) onCancel();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content
          ref={content}
          className="warning"
          aria-describedby={undefined}
          // A click outside answers nothing (`docs/ui-primitives.md`). Escape is Cancel.
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            content.current?.querySelector("input")?.focus();
          }}
        >
          <Dialog.Title>New persona</Dialog.Title>
          <p className="where">
            in <code>{plane}/personas/</code>, committed with the project
          </p>

          <form
            onSubmit={(event) => {
              event.preventDefault();
              create();
            }}
          >
            <SettingRow
              label="Name"
              help={
                <>
                  Lowercase letters, digits, <code>.</code>, <code>_</code> and <code>-</code>.
                </>
              }
              control={(ids) => <Field ids={ids} kind="text" value={name} onChange={setName} />}
            />

            <SettingRow
              label="Role"
              help="Optional. Left empty, it is the name, title-cased."
              control={(ids) => <Field ids={ids} kind="text" value={role} onChange={setRole} />}
            />

            <SettingRow
              label="Delegate when"
              help={
                "When the steward should route work here. Required unless it inherits from " +
                "another persona."
              }
              control={(ids) => (
                <Field
                  ids={ids}
                  kind="text"
                  value={when}
                  placeholder="CI/CD pipelines, k8s deploys, cluster access"
                  onChange={setWhen}
                />
              )}
            />

            <SettingRow
              label="Inherits from"
              help={
                offered === undefined
                  ? "Optional. Reading the project's personas…"
                  : "unread" in offered
                    ? `purlis could not read the project's personas: ${offered.unread}`
                    : "Optional: another persona of this project."
              }
              control={(ids) => (
                <Choice
                  ids={ids}
                  kind="select"
                  value={parent}
                  unset="None"
                  onValueChange={setParent}
                  options={(offered !== undefined && "names" in offered ? offered.names : []).map(
                    (one) => ({ value: one, label: one }),
                  )}
                />
              )}
            />

            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}

            <SettingActions>
              <button type="submit" tabIndex={0} disabled={!ready}>
                {making ? "Creating…" : "Create persona"}
              </button>
              <button type="button" tabIndex={0} disabled={making} onClick={onCancel}>
                Cancel
              </button>
            </SettingActions>
            {/* Why Create waits, said at the act rather than only in one row's help (#1719). */}
            {name.trim() !== "" && given(when) === null && given(parent) === null && (
              <p className="came-back">
                Create persona needs Delegate when, or a persona it inherits from.
              </p>
            )}
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
