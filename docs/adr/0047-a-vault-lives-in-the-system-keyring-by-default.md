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

## Amendment, 2026-09-24: a vault's identity token moves into the keyring (#237)

**The problem it closes.** A vault read through an identity variable (`"env":
{"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"}`) found its token in the environment of the
process running `charter secret`. So the token had to be exported in every chat's shell, where
any agent could `echo` it. One service-account token outweighs every secret it unlocks. The
operator's ruling (#232, decision 4): tokens move into the keyring, and no chat environment
carries an `OP_*` variable.

**Decision.**

- **The move.** A vault's tab in the app offers "Move this token into the Keychain" where the
  vault's identity variable is set in the app's environment. The core reads each variable the
  vault is read through from its own environment and stores it with `keyring::store(ctx)`, the
  one place the store is chosen, so a fenced build writes the stub. The item's service is
  `charter/identity` and its account is the variable's name. The core then writes
  `"identity": "keyring"` into the vault's config in this machine's registry half. Nothing is
  stored or marked unless every variable the vault declares is set: a half-moved identity would
  read one token from the keyring and look for the other in an environment that no longer
  carries it. The move writes an `identity-move` trace event that names the vault and the
  variables, never the token.
- **The lookup.** For a marked vault, `secrets::env_overlay` reads each identity variable from
  the keyring first and falls back to the environment. It falls back when the keyring holds
  nothing and when the keyring cannot be read, and a keyring error reaches the refusal only when
  the environment has nothing either. An unmarked vault never asks the keyring. `charter secret`
  from a plain terminal reads the moved token the same way, so it keeps working after the
  terminal stops exporting it.
- **The item is keyed by the variable's name, machine-wide,** unlike a vault's own items, which
  get a random service per vault. The variable is itself machine-wide: two planes whose vaults
  read `$OP_TEAM_TOKEN` were handed one token by one shell.
- **Only the local half can mark.** The mark is read from `.charter/vaults.json` alone. The
  committed `vaults.json` arrives by `git pull`, and a mark there would let a commit choose which
  keyring item charter hands to the `op` it runs next. A unit test pins this
  (`a_committed_registry_cannot_mark_an_identity_as_held_in_the_keyring`).
- **Where an identity is held is said from the mark**, never by reading the keyring:
  `secrets::identity::held` and `secrets::identity_missing`. So drawing the vault tab, the
  Vaults panel and `vault list` never makes the Keychain ask anything.
- **No chat carries an `OP_*` variable.** `Sessions::open` removes every `OP_*` name the app
  inherited from each chat's environment, as it removes a harness's identity, and drops any
  `OP_*` that a harness profile's `env` declares. The prefix is `secrets::identity::KEPT_FROM_CHATS`.
  All of them are removed, not only those a vault declares: an unregistered token is still a
  token, and `op` reads `$OP_SERVICE_ACCOUNT_TOKEN` on its own. The extension executor already
  starts its programs from an empty environment plus eight named variables, none of them `OP_*`.

**What it does not do.**

- A chat's shell runs the operator's startup files, and a `.zshrc` that exports the token puts it
  back. The docs say to delete that line after the move.
- A terminal outside the app keeps whatever it exports.
- The token still reaches `op`'s environment for each call, as it did before.
- Identity variables that are not `OP_*` (`VAULT_TOKEN`) can be moved, but a chat still inherits
  them from the app. *Superseded twice: the #271 amendment below strips every identity name a
  vault declares, and the 2026-09-29 amendment starts every chat from an allowlisted
  environment, so a variable the app inherited reaches a chat only when a keep-list names it.*
- On macOS the item is written by the app, so the first read by the `charter` command prompts
  once. "Always Allow" adds that program. This is the two-programs consequence above.
- Nothing moves a token back into the environment. Removing the item from the keyring (or the
  mark from `.charter/vaults.json`) returns the vault to reading the environment.

## Amendment, 2026-09-24: the token move hardened after the #271 review

An adversarial review of the token move (#271) found several ways a chat could still reach a
moved token, or redirect it. The move is kept, with these changes. Each is covered by a
regression test that began as a proof the exploit worked.

- **charter runs only the `op` it pinned.** A keyring-held identity is handed to the absolute
  `op` resolved from the operator's own PATH when the token was stored, and to that binary's
  code-signing Team identifier, both recorded in the local half. A read verifies the path and the
  team and refuses on a mismatch; it never resolves `op` from the caller's PATH. A chat that drops
  its own `op` on `$PATH` therefore cannot receive the token. The Team id recorded is whatever
  `codesign` reports at store time (trust on first use); AgileBits' published id is named in the
  code for an operator to confirm a fresh install against, not hard-checked. Without new `unsafe`:
  the check shells out to `/usr/bin/codesign`.
- **The keyring item is random per (vault, plane), and the binding is pinned.** The item is
  `charter/@identity/<random id>`, the id stored in the local half — so a chat cannot name another
  plane's item, and two planes that bind the same variable never share one. The local record also
  fixes the `env` map, the op-vault and the account the move was made against; the mark is honoured
  only while the vault's effective binding still equals them. A committed `vaults.json` that changes
  any of them unpins the mark rather than redirecting the token, which closes the earlier hole
  where a commit could pick the keyring item.
- **The whole record is local-only.** `identity` joins `account` in `LOCAL_ONLY_KEYS`, and it is
  read from the local half alone, so a committed entry can neither create a record nor change one.
- **A keys index may name only this vault's own service.** `charter/<vault>/<id>`, checked against
  the vault name — never `charter/@identity/…` or another vault's service — so an index cannot
  point a keyring vault at the identity item and read the token out as a secret.
- **The preferred move is a password box.** The vault tab lets the operator paste the token
  straight into the keyring, so it never enters the app's own environment. The move-from-environment
  path stays for convenience but the tab warns to relaunch, because a same-user process can read
  the app's environment block while the export is still in it.
- **The chat strip is by every declared name, case-insensitively.** No chat is given any `OP_*`
  variable (matched ignoring case) or any identity source a vault declares
  (`registry::identity_vars`), so a vault bound to `PROD_1P_TOKEN` leaks it to no chat either.

**A defence-in-depth gap is left open, tracked separately:** the app has no Tauri command
allow-list, so only the webview CSP stands between a future cross-site-scripting bug and the
reveal/copy commands. Filed as a follow-up. *Closed by ADR 0052 (charter-app#276): every app
command is on an allow-list, and reveal and copy are granted to the main window only, by a
capability of their own.*

## Amendment, 2026-09-29: a chat starts from an allowlisted environment

The strip above is a list of names charter knows to remove, so every other variable the app
inherited reached each chat and, through the harness, the model's shell. A chat now starts the
way the extension executor starts a program: from an empty environment plus a keep-list
(`charter_core::chatenv`).

- **The keep-list** is what a program needs to run as the operator (`PATH`, `HOME`, the locale,
  `TMPDIR`, `SSH_AUTH_SOCK`, `XDG_*`, the proxies, charter's own `CHARTER_*`), plus what the
  chat's harness declares for itself (`Harness::env_passed`: data per harness, so a new harness
  brings its own names), plus the operator's `[chat_env] pass` in the plane's
  `charter.local.toml`.
- **Credentials are held back by class.** Forge, cloud, model-provider and registry credentials,
  and any name a profile's `env` may not use (`KEY`, `TOKEN`, `SECRET`, `PASSWORD`), pass only
  when the operator lists the exact name. A prefix the operator writes does not admit one.
- **What is never passed stays that way.** Every `OP_*`, every identity variable a vault
  declares, a harness's identity and where the launcher's chat was, whoever lists them. `TERM`
  is always the chat's own.
- **Only the local file extends it.** A committed list would let a teammate's push decide what
  of one machine's environment every chat there gets, which is the rule `charter.local.toml`
  already carries for `[harness]`.

`charter secret exec` stays the way a command is given a credential. What the harness itself
then hands its tools is the harness's own policy.

## Amendment, 2026-10-04: every item is held to charter's app (ruling V90, #1179)

**The problem it closes.** A sandboxed project with a keyring vault started no chat. No harness's
sandbox could keep a chat off the operating system's credential store, so ADR 0067 §5's vaults
class could not be held, and both defaults together (sandbox on, keyring vaults) refused every
chat. The operator's ruling V90: hold the class at the store.

**Decision (V90a).** On macOS every keyring item charter writes is held to charter's app: only
the app's own binary reads it without the person's confirmation. Any other program, the
`charter` command and every program a chat runs included, is refused or makes the Keychain ask
the person first.

- **The rule is the Keychain's own default, applied by the app's binary.** An item made with no
  access object of its own trusts the program that created it, by its code signature (measured
  above). So the app writes every item itself. The `charter` command starts the app's binary
  beside it again, as a one-item writer that takes the item on its standard input, writes it
  and exits (`secrets::keyhold`). The writer writes and never reads, so starting it gives a
  caller nothing back. It writes only items under `charter/`.
- **An item is deleted and made again, never replaced.** A replaced item keeps the access it
  had, so one the person once let another program read ("Always Allow") would keep letting it.
  A program may delete only an item it owns, so an item the `charter` command made before is
  deleted by the command and then made by the app. If the app cannot make it, the command writes
  it back itself, and the item is tried again at its next read. The command blocks the interrupt,
  hang-up and terminate signals from its delete to the new write and delivers them after, so
  the value is lost there only if the command is killed in a way no program can block. The app
  and the writer leave their signals alone; the writer runs in a process group of its own.
- **The command finds the app through a link.** The app's Install on PATH puts a link to the
  command in `/usr/local/bin`; the app is looked for beside the file the link names, never
  beside the link.
- **The index records it.** Each key whose item was written held carries `"held": true` in the
  keys index (`docs/plane-format.md`).
- **No new `unsafe`, and no shell-out.** A trusted-application list naming the app
  (`SecAccessCreate`) would take an FFI block, and the `security` tool's `-T` would make that tool
  the item's writer. Writing it as the app needs neither.
- **The vaults class is then held for every harness on macOS** (ADR 0067 §5 as amended): a
  Claude Code chat starts, and Codex and opencode run inside charter's wrap, which also denies the
  credential store's service (V90b).

**Existing items (V90d).** An item whose index line does not say `held` is written again, held,
the first time charter reads it, and its value still resolves. Only the program that made an
item can delete it, so an item the `charter` command made is moved the next time the command
reads it; the app cannot move it. A move is made only while the key is as it was read, so it
never writes over a newer value. The vault's next `charter secret
get`, `cp` or `exec` says once that its secrets were moved under the rule. Until its first read,
an item keeps the access it had.

**Measured on macOS 26.2, 2026-10-04,** with throwaway programs against a scratch keychain made
with `security create-keychain` and deleted afterwards (the search list was unchanged), each read
with the Keychain's dialogs off. An item made by one program and read by:

| Reader | Result |
|---|---|
| The program that made it | read |
| Another program | refused (would ask) |
| Another program, from inside a live Claude Code chat's sandbox (2.1.288, a turn against a stand-in model) | refused (would ask); the maker, in the same chat, read it |
| Any program, under charter's own wrap (V90b) | refused: the service cannot be reached at all |

An item the `charter` command had made was refused to the app's delete and deleted by the
command, then made by the app, after which the command was refused and the app read it.

**What it rests on, and what it does not hold.**

- **Unsigned and ad-hoc signed builds are trusted by their exact build.** A dev build, and an
  update of today's ad-hoc signed app, is a new program to the Keychain, so its first read of
  each item asks the person once, and "Always Allow" adds it. This was chosen over pinning a
  path, which would trust whatever binary is put there. Signed builds (#606) are trusted by their
  designated requirement, which survives an update; a data-protection access group follows
  them.
- **Where no app sits beside the `charter` command** (a command-line install, or a build of the
  command alone), the command writes the item itself, and `charter secret set` says that the
  item is not held. It stays the command's until a command with the app beside it reads it.
- **After an update of an ad-hoc signed build, neither new binary owns the items an older build
  made**, so charter cannot move them again until signed builds. They stay held to the build
  that made them, and reads ask.
- **The `charter` command now asks too.** It is another program, so `charter secret get`,
  `exec` and `cp` make the Keychain ask the person before each read of a held item. Answering
  each ask keeps the rule. "Always Allow" for the command does not: it lets that build read the
  item without asking, and so lets any chat that runs the command read it silently. Resolving a
  secret in `charterd` (V16b, #1180) takes the command out of that path.
- **The rule is about reading.** It is not an integrity boundary for the item, and the item's
  name stays visible to other programs, as every Keychain item's does. The follow-ups are
  tracked in #1180.
- **Linux (V90c).** The Secret Service keeps no per-program rule, and no harness's sandbox was
  measured keeping a chat off the session bus, so a sandboxed project with a keyring vault still
  refuses a Claude Code chat there. The refusal says how to go on: start that chat without the
  sandbox from the new-chat picker, or, for a resumed or relaunched chat too, move those secrets to a plain-file or 1Password vault, which the sandbox can keep
  from a chat. Codex and opencode wait for charter's Linux wrap (#1040).
