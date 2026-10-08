### Added

- **Making a 1Password vault asks how it signs in, tests it, and keeps the token in the
  Keychain.** *New vault* with 1Password chosen is a short set-up: a service-account token
  pasted once, or the 1Password app on this machine with the account or sign-in address
  chosen; the 1Password vault picked from the ones that sign-in can see; a test that reads
  item names only; then one step that registers the vault and stores the token. No shell
  export and no restart. A token used by several vaults can be ticked for the others, each
  shown with what would be pinned for it. A vault's tab has *Change how this vault signs in*,
  which also converts a vault bound to an environment variable. From a terminal:
  `purlis vault add <name> --provider 1password --op-vault <NAME> --token-stdin` (#1527).

### Security

- **A vault's token is given to purlis itself and is kept in the system keyring alone.** A
  vault set up this way binds no environment variable, so there is no export for a chat's
  shell to read. The keyring record now also pins the item a vault keeps its secrets in.
  `vault add --token-stdin` refuses inside a chat before it reads anything (#1527, ADR 0047's
  2026-10-09 amendment).
