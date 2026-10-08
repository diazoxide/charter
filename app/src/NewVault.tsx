import { useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import type { SetupDone } from "./bindings";
import { Choice, Field, SettingActions, SettingRow } from "./settings/components";
import { VaultSignIn } from "./VaultSignIn";

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
    says: "Items in a 1Password vault, read through the op command. Asks how purlis signs in, and tests it.",
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
 * **A 1Password vault is a short guided set-up** (#1527, {@link VaultSignIn}): how purlis signs
 * in, where the items live, a test, and then one step that registers the vault and stores its
 * token. That part talks to the core itself, because a token is handed over once and held
 * there, never here; it answers through `onMade` with the vault as it now is.
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
  onMade,
  onCancel,
}: {
  /** Where the vault is registered, so the dialog says so. */
  plane: string;
  /** Why the last attempt made nothing — the core's sentence, unchanged. */
  trouble?: string;
  /** Whether charter is making it right now, so the answer cannot be given twice. */
  making: boolean;
  onCreate: (name: string, provider: string, opVault: string | null) => void;
  /** A 1Password vault was made by the guided set-up: the vault as it now is, and what became
   *  of the other vaults ticked for its token. */
  onMade: (done: SetupDone) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [provider, setProvider] = useState<string>("keyring");
  /** Whether the guided set-up is at work in the core. */
  const [settingUp, setSettingUp] = useState(false);
  const guided = provider === "1password";
  const busy = making || settingUp;
  const ready = name.trim() !== "" && !guided && !making;
  const create = () => {
    if (ready) onCreate(name.trim(), provider, null);
  };
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        // Not while charter is making it: a vault made behind a closed dialog would open a tab
        // nobody asked to see, and a refusal would land where nobody is looking.
        if (!open && !busy) onCancel();
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
                  disabled={settingUp}
                />
              )}
            />

            {guided ? (
              <VaultSignIn
                plane={plane}
                name={name}
                onBusy={setSettingUp}
                onDone={onMade}
                onCancel={onCancel}
              />
            ) : (
              <>
                {trouble && (
                  <p className="trouble" role="alert">
                    {trouble}
                  </p>
                )}

                <SettingActions>
                  <button type="submit" tabIndex={0} disabled={!ready}>
                    Create vault
                  </button>
                  <button type="button" tabIndex={0} disabled={making} onClick={onCancel}>
                    Cancel
                  </button>
                </SettingActions>
              </>
            )}
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
