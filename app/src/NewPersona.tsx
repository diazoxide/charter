import { useId, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";

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
  const nameId = useId();
  const roleId = useId();
  const whenId = useId();
  const parentId = useId();
  const box = useRef<HTMLInputElement>(null);
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
          className="warning"
          aria-describedby={undefined}
          // A click outside answers nothing (`docs/ui-primitives.md`). Escape is Cancel.
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            box.current?.focus();
          }}
        >
          <Dialog.Title>New persona</Dialog.Title>
          <p className="where">
            in <code>{plane}/personas/</code>, committed with the plane
          </p>

          <form
            className="asks"
            onSubmit={(event) => {
              event.preventDefault();
              create();
            }}
          >
            <label htmlFor={nameId}>Name</label>
            <input
              id={nameId}
              ref={box}
              value={name}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setName(event.target.value)}
            />
            <p className="came-back">
              Lowercase letters, digits, <code>.</code>, <code>_</code> and <code>-</code>.
            </p>

            <label htmlFor={roleId}>Role</label>
            <input
              id={roleId}
              value={role}
              autoComplete="off"
              placeholder="The name, title-cased"
              onChange={(event) => setRole(event.target.value)}
            />

            <label htmlFor={whenId}>Delegate when</label>
            <input
              id={whenId}
              value={when}
              autoComplete="off"
              placeholder="CI/CD pipelines, k8s deploys, cluster access"
              onChange={(event) => setWhen(event.target.value)}
            />
            <p className="came-back">
              When the steward should route work here. Required unless it inherits from another
              persona.
            </p>

            <label htmlFor={parentId}>Inherits from</label>
            <input
              id={parentId}
              value={parent}
              autoComplete="off"
              spellCheck={false}
              placeholder="Optional: another persona's name"
              onChange={(event) => setParent(event.target.value)}
            />

            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}

            <div className="doing">
              <button type="submit" tabIndex={0} disabled={!ready}>
                Create persona
              </button>
              <button type="button" tabIndex={0} disabled={making} onClick={onCancel}>
                Cancel
              </button>
            </div>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
