# A vault lives in the system keyring by default

charter had three places to keep a vault's secrets: a plaintext JSON file (`plain-file`),
URIs resolved through somebody else's CLI (`reference`), and one 1Password item per vault
(`1password`). The only one that needs nothing installed was the plaintext file. The operator,
2026-09-24: *"i thing we need to support MacosKeychain as a vault"*, and after the grilling
(#232): an OS-keyring provider, **the default for new vaults**, with 1Password, plain-file and
reference staying as they are.

## Decision

**A `keyring` provider, built on the `keyring` crate** (4.x, its default `v1` feature). The
crate is the standard Rust binding to the three stores and picks the platform's own:
the login Keychain on macOS, the Secret Service on Linux, and the Credential Manager on Windows
(which charter does not ship yet, ADR 0031). `charter vault add <name>` with no `--provider`
registers a keyring vault.

**One item per secret.** Service `charter/<vault>/<8 hex>`, account `<key>`, value the secret.
The 8 hex characters are random and made at the vault's first write. A vault name is only unique
within one plane, but the keyring belongs to the whole machine. Two planes that each have a vault
called `ops` would otherwise read, overwrite and delete each other's items, while each plane's
index listed only its own keys. A unit test pins this
(`two_planes_with_a_vault_of_one_name_never_read_each_others_items`).

**A keys index, because a keyring cannot be listed.** The `keyring` crate's portable API reads,
writes and deletes one named entry and has no call that lists entries. So
`.charter/vaults/<vault>.keys.json` holds:

- the service;
- each key's name;
- each key's size band (`16–31 bytes`), never its length;
- when each key was last written.

It never holds a value. It is 0600 and lives under `.charter/`, which git ignores. A vault's
service is written to it before the vault's first item is, and each key's line after its item
was written or deleted. A failure between the two leaves a key the index lists and the keyring
does not hold, which `vault verify` names, and never an item no index can find. Two charters
writing one vault at the same moment are not locked against each other, as a plain-file vault is
not: the index can lose a key whose item is still in the keyring. `secret list`, `vault list` and `secret audit` read
only the index, so they never touch the keyring and never make the Keychain ask anything.
`get`, `exec`, `cp` and `vault verify` read the item. `vault verify` names a key the index holds
and the keyring does not, for an item deleted behind charter's back.

**The index is refused if it points outside charter.** A service that does not start `charter/`
is treated as a corrupt index. The file is on disk, and an index naming another program's item
would make charter a deputy for reading it.

**No test reaches the operator's keychain.** Which store a build uses is decided in one place
(`secrets::keyring::store`). A fenced build (every `cargo test` build, and the app's `e2e` build,
`crate::fence`) keeps values in `keyring-stub.json` in the state directory of the plane it was given. A
test cannot reach the real store however it is written. The recorded-behaviour rows for the
keyring provider (`keyring-*` in `tests/fixtures/recorded/behaviour.jsonl`) run against that
stub. They were recorded from this build, because no Python charter had the provider.

The rules #227's review set for every provider hold here too:

- no value in any output a model can read, and none in any error;
- a keyring error names the operation, the service and the store's reason, and never formats
  `BadEncoding`, whose payload is the stored bytes;
- `Secret`, `FileStore` and `OsStore` have hand-written `Debug`s that print no value;
- a key name that is empty or holds a control character is refused before the store is asked;
- vault names are validated as before.

## What the macOS access restriction achieves, measured

The ruling asked that on macOS an item be readable without a prompt only by charter's own
binary, where the API allows it. **The default access already does exactly that, and it is the
most the safe API allows.**

`keyring`'s macOS store (`apple-native-keyring-store`) creates an item with
`SecKeychain::set_generic_password`, which is `SecKeychainAddGenericPassword` with no initial
access object. The Keychain then gives the item its default ACL: `decrypt` is granted to the
creating program alone, identified by its **code signature**, not by its path.

Measured on macOS 26.2 (25C56), 2026-09-24. Two throwaway programs were built against
`security-framework` 3.7.0, the version in this lockfile. They used a throwaway keychain file
made with `SecKeychainCreate` and deleted afterwards, which left the user's keychain search list
unchanged. Each read ran with `SecKeychainSetUserInteractionAllowed(false)`, so a read that would
prompt fails instead of putting a dialog on screen:

| Who reads the item | Result |
|---|---|
| The program that wrote it, in a later run | read, no prompt |
| A byte-identical copy of that program at another path | read, no prompt |
| Another program (a second binary) | refused, `-25293` (would prompt) |
| The writer rebuilt with a one-line source change (a new ad-hoc signature) | refused, `-25293` (would prompt) |
| Another program reading a control item made with `security add-generic-password -A` (any application) | read, no prompt |

The control row shows that the refusal comes from the item's ACL and not from anything else in
the setup. `security dump-keychain -a` on the item shows one application for
`decrypt derive export_clear export_wrapped mac sign`: the writer, with
`requirement: cdhash H"…"`. `change_acl` lists no application.

**What that protects, and what it does not.**

- **It protects against another program reading the value silently.** An agent running
  `security find-generic-password -w -s charter/…` makes the Keychain ask the operator, and the
  operator sees which program is asking.
- **It does not protect against charter itself being asked.** An agent that can run
  `charter secret get <vault> <key> --reveal --force` or `charter secret exec` is using the
  trusted program. Those commands' own rules still hold: `--reveal` needs a terminal or
  `--force`, `exec` redacts, and every hand-out is traced.
- **The app and the `charter` command are two programs.** An item written by one prompts the
  first time the other reads it. "Always Allow" adds the second to the ACL.
- **A charter update is a new program.** An ad-hoc signed build is trusted by its cdhash, so
  the first read after an update prompts again. A Developer ID signed build is trusted by its
  designated requirement (identifier and team), which survives an update. That was not measured
  here, because no build of this repository was Developer ID signed on the day.
- **Linux has no per-program access control.** By the Secret Service's design (not measured
  here), an unlocked collection's items go to any process in the user's session that asks over
  D-Bus. A keyring vault there is as safe as the session. That is still better than a plaintext
  file, which any process running as the user can read, including while the session is locked.

**What was not done, and why.** Trusting both charter binaries on one item needs
`SecAccessCreate` with a list of `SecTrustedApplicationCreateFromPath`. `security-framework`
exposes neither safely, so it would take an `unsafe` FFI block, and this repository allows
exactly one (CLAUDE.md). `kSecAttrAccessControl` (user presence, biometry) applies only to the
data-protection keychain, which requires a keychain-access-group entitlement that the
command-line binary cannot carry.

## Consequences

- A new vault is no longer a plaintext file unless the operator asks for one with
  `--provider plain-file`. The hints that used to suggest `--provider plain-file --file <path>`
  now suggest `charter vault add <name>`, and the recorded rows that pinned them were
  re-recorded on purpose. `vault-add-registers-a-plain-file-vault-locally` now passes
  `--provider plain-file`, the registration it always pinned.
- `secret audit` covers a keyring vault, from the index's `updated` times.
- `vault remove` on a keyring vault leaves its items in the keyring and its index on disk, as
  it leaves a plain-file vault's file, and says so: registering the name again as a keyring
  vault finds them.
- The index is the only record of a vault's service and keys. Losing it strands the items,
  which are still in Keychain Access under `charter/<vault>/…`.
