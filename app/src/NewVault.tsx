import { useId, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import * as RadioGroup from "@radix-ui/react-radio-group";

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
  const nameId = useId();
  const opId = useId();
  const pickId = useId();
  const box = useRef<HTMLInputElement>(null);
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
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            box.current?.focus();
          }}
        >
          <Dialog.Title>New vault</Dialog.Title>
          <p className="where">
            on <code>{plane}</code>, for this machine only
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
              Letters, digits, <code>.</code>, <code>_</code> and <code>-</code>.
            </p>

            <label id={pickId}>Kept in</label>
            <RadioGroup.Root
              className="choices"
              name="provider"
              value={provider}
              onValueChange={setProvider}
              aria-labelledby={pickId}
            >
              {PROVIDERS.map((one) => (
                <div className="choice" key={one.id}>
                  <RadioGroup.Item
                    className="dot"
                    value={one.id}
                    id={`${pickId}-${one.id}`}
                    aria-describedby={`${pickId}-${one.id}-says`}
                  >
                    <RadioGroup.Indicator className="dot-mark" />
                  </RadioGroup.Item>
                  <label className="who" htmlFor={`${pickId}-${one.id}`}>
                    {one.name}
                  </label>
                  <span className="meta" id={`${pickId}-${one.id}-says`}>
                    <span className="what">{one.says}</span>
                  </span>
                </div>
              ))}
            </RadioGroup.Root>

            {needsOp && (
              <>
                <label htmlFor={opId}>1Password vault</label>
                <input
                  id={opId}
                  value={opVault}
                  autoComplete="off"
                  spellCheck={false}
                  onChange={(event) => setOpVault(event.target.value)}
                />
                <p className="came-back">Where charter creates this vault's items.</p>
              </>
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
