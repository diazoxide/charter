/**
 * **The providers a vault can be kept by, named once** (#1719): New vault offers them under
 * these names, and the Vaults panel and a vault's tab say a vault's provider the same way, not
 * by the core's id for it (`keyring`, `plain-file`).
 *
 * In the order New vault offers them, with what each means. **The system keychain first and
 * chosen**, because #232 made it the default: a secret there is one item of the operating
 * system's own store, and nothing is written into the plane.
 */
export const PROVIDERS = [
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
    says: "A plaintext file under the project's state directory, which git never sees.",
  },
  {
    id: "reference",
    name: "References",
    says: "op:// references rather than values, so the file is safe to commit.",
  },
] as const;

/** A provider as the window names it; an id it does not know, as the core wrote it. */
export function providerName(id: string): string {
  return PROVIDERS.find((one) => one.id === id)?.name ?? id;
}
