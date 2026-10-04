import { useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { Choice, Field, SettingRow } from "./settings/components";

/**
 * The providers a new vault can be kept by, in the order they are offered, with what each means
 * to the operator. **The system keychain first and chosen**, because #232 made it the default:
 * a secret there is one item of the operating system's own store, and nothing is written into
 * the plane.
 */
const PROVIDERS = [
  {
    id: "keyring",
    name: "System keychain",
    says: "The macOS Keychain, or the Secret Service on Linux. Each secret is its own item.",
  },
  {
    id: "1password",
    name: "1Password",
    says: "Items in a 1Password vault, read through the op command.",
  },
  {
    id: "plain-file",
    name: "Plain file",
    says: "A plaintext file under the plane's state directory, which git never sees.",
  },
  {
    id: "reference",
    name: "References",
    says: "op:// references rather than values, so the file is safe to commit.",
  },
] as const;

/**
 * Making a vault, asked where the answer is given — `NewWorkspace`'s shape, for `charter vault
 * add <name> --provider <provider>`.
 *
 * **This dialog validates nothing** but that the question has been answered: what a vault may be
 * called, whether one is already registered by that name, whether a plaintext file would be
 * committed — all of it is `vaultcmd::add`'s, reached through `vault_create`, so the window and a
 * terminal refuse the same things in the same words.
 *
 * **Drawn from the settings set** (DS-3c, #1175; ADR 0037's 2026-10-04 amendment): each answer
 * is a {@link SettingRow} holding a {@link Field} or a {@link Choice}, so its line of help is
 * the box's own description, and the dialog looks like every other place charter asks.
 */
export function NewVault({
  plane,
  trouble,
  making,
  onCreate,
  onCancel,
}: {
  /** Where the vault is registered, so the dialog says so. */
  plane: string;
  /** Why the last attempt made nothing — the core's sentence, unchanged. */
  trouble?: string;
  /** Whether charter is making it right now, so the answer cannot be given twice. */
  making: boolean;
  onCreate: (name: string, provider: string, opVault: string | null) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [provider, setProvider] = useState<string>("keyring");
  const [opVault, setOpVault] = useState("");
  const needsOp = provider === "1password";
  const ready = name.trim() !== "" && (!needsOp || opVault.trim() !== "") && !making;
  const create = () => {
    if (ready) onCreate(name.trim(), provider, needsOp ? opVault.trim() : null);
  };
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        // Not while charter is making it: a vault made behind a closed dialog would open a tab
        // nobody asked to see, and a refusal would land where nobody is looking.
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
          // Radix focuses the first box as it opens, which is the name: nothing to override.
        >
          <Dialog.Title>New vault</Dialog.Title>
          <p className="where">
            on <code>{plane}</code>, for this machine only
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
                  Letters, digits, <code>.</code>, <code>_</code> and <code>-</code>.
                </>
              }
              control={(ids) => <Field kind="text" ids={ids} value={name} onChange={setName} />}
            />

            <SettingRow
              label="Kept in"
              grouped
              control={(ids) => (
                <Choice
                  kind="radio"
                  ids={ids}
                  options={PROVIDERS.map((one) => ({
                    value: one.id,
                    label: one.name,
                    says: one.says,
                  }))}
                  value={provider}
                  onValueChange={setProvider}
                />
              )}
            />

            {needsOp && (
              <SettingRow
                label="1Password vault"
                help="Where charter creates this vault's items."
                control={(ids) => (
                  <Field kind="text" ids={ids} value={opVault} onChange={setOpVault} />
                )}
              />
            )}

            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}

            <div className="doing">
              <button type="submit" tabIndex={0} disabled={!ready}>
                Create vault
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
