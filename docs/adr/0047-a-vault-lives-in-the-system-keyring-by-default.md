# A vault lives in the system keyring by default

purlis had three places to keep a vault's secrets: a plaintext JSON file (`plain-file`),
URIs resolved through somebody else's CLI (`reference`), and one 1Password item per vault
(`1password`). The only one that needs nothing installed was the plaintext file. The operator,
2026-09-24: *"i thing we need to support MacosKeychain as a vault"*, and after the grilling
(#232): an OS-keyring provider, **the default for new vaults**, with 1Password, plain-file and
reference staying as they are.

## Decision

**A `keyring` provider, built on the `keyring` crate** (4.x, its default `v1` feature). The
crate is the standard Rust binding to the three stores and picks the platform's own:
the login Keychain on macOS, the Secret Service on Linux, and the Credential Manager on Windows
(which purlis does not ship yet, ADR 0031). `purlis vault add <name>` with no `--provider`
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
and the keyring does not, for an item deleted behind purlis's back.

**The index is refused if it points outside purlis.** A service that does not start `charter/`
is treated as a corrupt index. The file is on disk, and an index naming another program's item
would make purlis a deputy for reading it.

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

The ruling asked that on macOS an item be readable without a prompt only by purlis's own
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
- **It does not protect against purlis itself being asked.** An agent that can run
  `purlis secret get <vault> <key> --reveal --force` or `purlis secret exec` is using the
  trusted program. Those commands' own rules still hold: `--reveal` needs a terminal or
  `--force`, `exec` redacts, and every hand-out is traced.
- **The app and the `purlis` command are two programs.** An item written by one prompts the
  first time the other reads it. "Always Allow" adds the second to the ACL.
- **A purlis update is a new program.** An ad-hoc signed build is trusted by its cdhash, so
  the first read after an update prompts again. A Developer ID signed build is trusted by its
  designated requirement (identifier and team), which survives an update. That was not measured
  here, because no build of this repository was Developer ID signed on the day.
- **Linux has no per-program access control.** By the Secret Service's design (not measured
  here), an unlocked collection's items go to any process in the user's session that asks over
  D-Bus. A keyring vault there is as safe as the session. That is still better than a plaintext
  file, which any process running as the user can read, including while the session is locked.

**What was not done, and why.** Trusting both purlis binaries on one item needs
`SecAccessCreate` with a list of `SecTrustedApplicationCreateFromPath`. `security-framework`
exposes neither safely, so it would take an `unsafe` FFI block, and this repository allows
exactly one (CLAUDE.md). `kSecAttrAccessControl` (user presence, biometry) applies only to the
data-protection keychain, which requires a keychain-access-group entitlement that the
command-line binary cannot carry.

## Consequences

- A new vault is no longer a plaintext file unless the operator asks for one with
  `--provider plain-file`. The hints that used to suggest `--provider plain-file --file <path>`
  now suggest `purlis vault add <name>`, and the recorded rows that pinned them were
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
process running `purlis secret`. So the token had to be exported in every chat's shell, where
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
  the environment has nothing either. An unmarked vault never asks the keyring. `purlis secret`
  from a plain terminal reads the moved token the same way, so it keeps working after the
  terminal stops exporting it.
- **The item is keyed by the variable's name, machine-wide,** unlike a vault's own items, which
  get a random service per vault. The variable is itself machine-wide: two planes whose vaults
  read `$OP_TEAM_TOKEN` were handed one token by one shell.
- **Only the local half can mark.** The mark is read from `.charter/vaults.json` alone. The
  committed `vaults.json` arrives by `git pull`, and a mark there would let a commit choose which
  keyring item purlis hands to the `op` it runs next. A unit test pins this
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
- On macOS the item is written by the app, so the first read by the `purlis` command prompts
  once. "Always Allow" adds that program. This is the two-programs consequence above.
- Nothing moves a token back into the environment. Removing the item from the keyring (or the
  mark from `.charter/vaults.json`) returns the vault to reading the environment.

## Amendment, 2026-09-24: the token move hardened after the #271 review

An adversarial review of the token move (#271) found several ways a chat could still reach a
moved token, or redirect it. The move is kept, with these changes. Each is covered by a
regression test that began as a proof the exploit worked.

- **purlis runs only the `op` it pinned.** A keyring-held identity is handed to the absolute
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
reveal/copy commands. Filed as a follow-up. *Closed by ADR 0052 (purlis#276): every app
command is on an allow-list, and reveal and copy are granted to the main window only, by a
capability of their own.*

## Amendment, 2026-09-29: a chat starts from an allowlisted environment

The strip above is a list of names purlis knows to remove, so every other variable the app
inherited reached each chat and, through the harness, the model's shell. A chat now starts the
way the extension executor starts a program: from an empty environment plus a keep-list
(`charter_core::chatenv`).

- **The keep-list** is what a program needs to run as the operator (`PATH`, `HOME`, the locale,
  `TMPDIR`, `SSH_AUTH_SOCK`, `XDG_*`, the proxies, purlis's own `CHARTER_*`), plus what the
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

`purlis secret exec` stays the way a command is given a credential. What the harness itself
then hands its tools is the harness's own policy.

## Amendment, 2026-10-04: every item is held to purlis's app (ruling V90, #1179)

**The problem it closes.** A sandboxed project with a keyring vault started no chat. No harness's
sandbox could keep a chat off the operating system's credential store, so ADR 0067 §5's vaults
class could not be held, and both defaults together (sandbox on, keyring vaults) refused every
chat. The operator's ruling V90: hold the class at the store.

**Decision (V90a).** On macOS every keyring item purlis writes is held to purlis's app: only
the app's own binary reads it without the person's confirmation. Any other program, the
`purlis` command and every program a chat runs included, is refused or makes the Keychain ask
the person first.

- **The rule is the Keychain's own default, applied by the app's binary.** An item made with no
  access object of its own trusts the program that created it, by its code signature (measured
  above). So the app writes every item itself. The `purlis` command starts the app's binary
  beside it again, as a one-item writer that takes the item on its standard input, writes it
  and exits (`secrets::keyhold`). The writer writes and never reads, so starting it gives a
  caller nothing back. It writes only items under `charter/`.
- **An item is deleted and made again, never replaced.** A replaced item keeps the access it
  had, so one the person once let another program read ("Always Allow") would keep letting it.
  A program may delete only an item it owns, so an item the `purlis` command made before is
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
  Claude Code chat starts, and Codex and opencode run inside purlis's wrap, which also denies the
  credential store's service (V90b).

**Existing items (V90d).** An item whose index line does not say `held` is written again, held,
the first time purlis reads it, and its value still resolves. Only the program that made an
item can delete it, so an item the `purlis` command made is moved the next time the command
reads it; the app cannot move it. A move is made only while the key is as it was read, so it
never writes over a newer value. The vault's next `purlis secret
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
| Any program, under purlis's own wrap (V90b) | refused: the service cannot be reached at all |

An item the `purlis` command had made was refused to the app's delete and deleted by the
command, then made by the app, after which the command was refused and the app read it.

**What it rests on, and what it does not hold.**

- **Unsigned and ad-hoc signed builds are trusted by their exact build.** A dev build, and an
  update of today's ad-hoc signed app, is a new program to the Keychain, so its first read of
  each item asks the person once, and "Always Allow" adds it. This was chosen over pinning a
  path, which would trust whatever binary is put there. Signed builds (#606) are trusted by their
  designated requirement, which survives an update; a data-protection access group follows
  them.
- **Where no app sits beside the `purlis` command** (a command-line install, or a build of the
  command alone), the command writes the item itself, and `purlis secret set` says that the
  item is not held. It stays the command's until a command with the app beside it reads it.
- **After an update of an ad-hoc signed build, neither new binary owns the items an older build
  made**, so purlis cannot move them again until signed builds. They stay held to the build
  that made them, and reads ask.
- **The `purlis` command now asks too.** It is another program, so `purlis secret get`,
  `exec` and `cp` make the Keychain ask the person before each read of a held item. Answering
  each ask keeps the rule. "Always Allow" for the command does not: it lets that build read the
  item without asking, and so lets any chat that runs the command read it silently. Resolving a
  secret in `purlisd` (V16b, #1180) takes the command out of that path.
- **The rule is about reading.** It is not an integrity boundary for the item, and the item's
  name stays visible to other programs, as every Keychain item's does. The follow-ups are
  tracked in #1180.
- **Linux (V90c).** The Secret Service keeps no per-program rule, and no harness's sandbox was
  measured keeping a chat off the session bus, so a sandboxed project with a keyring vault still
  refuses a Claude Code chat there. The refusal says how to go on: start that chat without the
  sandbox from the new-chat picker, or, for a resumed or relaunched chat too, move those secrets to a plain-file or 1Password vault, which the sandbox can keep
  from a chat. Codex and opencode wait for purlis's Linux wrap (#1040).

## Amendment, 2026-10-09: a token is given to purlis, and a tick extends it (#1527)

**The problem it closes.** A vault read with a service-account token was still set up by hand:
a variable's name in the registry, an export in a shell profile that every chat's shell could
read, and later a move into the keyring. And a token used by several vaults had to be pasted
once per vault, because the first cut of #1526 that marked every alike vault did it silently,
from the merged registry, and was taken out.

**Decision.**

- **A vault may declare a token with no variable.** `"token": "keyring"` in a 1Password
  vault's config says it is read with a service-account token purlis keeps in the keyring. It
  names no secret, no variable and no keyring item, so it may be committed. It reads as one
  binding whose source is a fixed word that is no variable's name and is never looked up in an
  environment, and an `env` binding of the token's variable beside it is not honoured. So
  nothing in either half of the registry can name where such a token is read from.
- **The registration and the record are one step.** *New vault* in the app, *Change how this
  vault signs in* on a vault's tab and `purlis vault add --token-stdin` store the token's item,
  then write this machine's half once, with the vault and its pinned record together. The vault
  is then read back as both halves merge it; unless its settings are exactly the ones the
  person gave and the record is honoured for them, everything is undone. A failure anywhere
  deletes the item and puts both halves back.
- **The record also pins the item** the vault keeps its secrets in, beside the binding, the
  1Password vault, the account and the provider's program. A record made before this amendment
  names no item and is not held to one.
- **A sign-in is tested before anything is registered**, by reading item names only. A failed
  test is said in purlis's own fixed sentences, by kind; the provider's output is never shown.
- **One token may be given to several vaults, by the person's tick.** After a token is given,
  the set-up lists the other vaults bound to the same identity, each with the settings a record
  would pin for it, which half of the registry names it, and a digest of those. A vault the
  committed half names starts unticked. The store takes the ticked names with the digests the
  person was shown, recomputes each, and marks only those that still match, each under its own
  keyring item and its own record; the rest are skipped and said.

**What stays as the 2026-09-24 amendment wrote it.** A committed entry can neither create a
record nor change one. What makes a record for a vault the committed half names is the person's
tick, in the app's own window, on the settings they were shown; a commit that changes those
settings after they were shown unpins the tick's digest, and a commit that changes them after
the record was made unpins the record. A store with no ticks marks the one vault it is for, as
before.

**A chat never supplies a vault's token.** The window's commands are not a chat's to invoke.
`vault add --token-stdin` refuses inside a chat, or a shell the app started, before it reads
anything: by the marks the app sets in the environment of what it starts, and by whether the
command runs below a program the project's record of open chats names, so clearing the
variables does not pass. That check is a courtesy to an agent, not the boundary. The boundary
is the sandbox's: both halves of the registry and the keyring are not a sandboxed chat's to
write (ADR 0067). A chat that runs with no sandbox can write whatever the person can, as before.

**Not decided here.** A 1Password Connect server (a host and a token) is not supported by the
provider and is not offered. An identity read from an environment variable stays supported for
a machine with no keyring and for CI.

## Amendment, 2026-10-09: a record made before the item was pinned (#1542)

**Correcting the line above** that a record made before the 2026-10-09 amendment "names no item
and is not held to one". Such a record is now held to an item at its next read, and how depends
on who chose the item the vault reads:

- **This machine chose it** (its half's own `op-item`, or the default `charter-<name>` when no
  half names one): the read writes that item into the record, quietly, and from then on the
  record is held to it like one made since.
- **The committed half chose it** (the item the merged registry names differs from the one this
  machine's half alone gives): the record is not honoured and is never pinned. The read is
  refused with a sentence that points to the vault's tab, where the token is given again on the
  settings shown.

The two are told apart by the item each view reads, never by which keys are present. This keeps
the rule above as written: a committed entry can neither create a record nor change one.

## Amendment, 2026-10-10: one Keychain question per run, and none from a sandboxed chat (#1638)

**The problem it closes.** With a vault's service-account token kept in the keyring, the
`purlis` command made the Keychain ask once per `op` it ran, and `secret exec` runs `op` once per
value it hands on: three `--env` values were three questions in one run. Several chats running
`secret exec` raised a stream of them, each "Allow" good for one read. And where the app did not
answer a sandboxed chat's `secret exec`, or for `secret get` and `cp`, which are not handed to the
app, the command read the Keychain itself from inside the chat (V90's measurement: such a read
asks the person).

**Decision (delegated, 2026-10-10).**

- **One run reads each kept token once.** The context a command reads through remembers the
  token it read, for that run only. It is never kept across commands, and the app builds a new
  context for every request it serves, so a replaced or deleted token is read afresh by the
  next run.
- **A sandboxed chat never reads the keyring through the command.** In a chat the app started
  sandboxed, a vault whose values come from the keyring (a `keyring` vault, or one whose token is
  kept there) is refused rather than read: by `secret exec` when no app takes the run, by
  `secret get` and `cp`, which the app does not broker, and, as a backstop where the keyring
  itself is read, by every other read (`vault verify`, `secret rename`, a listing that runs
  `op`). The sentence says why and what to run instead: on macOS `secret exec` through the
  app; where the app runs no command for a chat yet (Linux, #1040), a terminal outside the
  chat. A vault the keyring does not hold runs as it did. On Linux, where the Secret Service
  answers without asking, it refuses the same way: there a sandboxed chat's `secret get`, `cp`
  and every other read of such a vault, which used to read it, are now refused.
- **`vault list` never reads a kept token to draw its status** (#1180). A 1Password vault whose
  token is kept in the keyring shows that it is kept there and that `vault verify` reads it,
  as the guide already said; it no longer runs `op`, which made the Keychain ask once per such
  vault in every listing. `persona list` and `vault add` still check the vault live (#1180).
- **A courtesy, not the boundary.** The refusal reads the chat's own environment, which the
  chat can change. What holds the item is still the store's rule (V90a) and the sandbox; this
  keeps purlis from raising a question on the chat's behalf and says what to do.

**Not decided here (V16b, #1180).** A chat started without the sandbox, and a terminal, still
read the Keychain through the command, so each run asks once. Two ways to take the command out
of that path, each the operator's call:

- **The app runs the command**, as it does for a sandboxed chat: for an unsandboxed chat that
  is a command run outside any sandbox on a chat's word, and it holds such chats to their
  persona's vaults for the first time.
- **The app hands the token back** over the socket and the command runs `op` as now: no new
  run path, but a raw secret then reaches a chat's process, which no sandboxed chat may hold
  (ADR 0067 §5 class 1), and any process of the person's that can reach the socket could ask.

A terminal has no chat token and `local-ui` is the window's alone (V16a), so either way it
needs a route of its own.

## Amendment, 2026-10-10: one Keychain question per item per run of the app (#1654)

**The problem it closes.** After an update of an ad-hoc signed build, reopening the app with
several chats raised a stream of Keychain questions for one kept token. The amendment above made
each command read a kept token once, but the app built a new context for every request it
served, so each brokered `secret exec` and each vault tab read the item again, and the Vaults
panel, which every project window draws as it opens, ran `op` with the token to draw its status.

**Decision (delegated, 2026-10-10; flagged for the operator).**

- **The app reads each kept token once per run.** Every context the app builds shares the app's
  one memory of the kept tokens it read. It lives in the app's process only. It is never
  written anywhere and never handed to a chat: a brokered `secret exec` hands the token to `op`
  as before, and the chat gets the command's output. The app's next run reads afresh. This
  replaces "the app builds a new context for every request it serves" above, for the memory
  only.
- **It fails closed where purlis can see a change.** The vault's record of its kept token is
  checked again before every use, so a token removed through purlis is never answered from
  memory, a token put in again is a new item, read afresh, and an item purlis deletes is
  dropped from memory with it. **What it does not see:** a token
  deleted or edited directly in Keychain Access while the app runs is still used until the app
  quits. Revoking the token in 1Password stops it either way. That gap is the widening, and the
  operator rules on it: keep it, or re-check that the item still exists before each use.
- **No listing reads a kept token** (#1180): `persona list`, `persona show` and the app's Vaults
  panel draw the same status `vault list` does, from the record. `vault add` still checks the
  vault live, because the person just asked for it. This replaces "`persona list` and
  `vault add` still check the vault live" above. The app's Vaults panel runs no `op` at all to
  draw a 1Password vault's row, since a vault that signs in through the 1Password app reads
  that app's data; the vault's tab reads it.
- **An `op` handed a service-account token is told not to read the 1Password app's settings**
  (`OP_LOAD_DESKTOP_APP_SETTINGS=false`). It read them from the 1Password app's container on
  every run, which made macOS ask whether purlis may "access data from other apps". A vault that
  signs in through the 1Password app reads them as before.

The lasting fix for questions after every update is a stable code-signing identity (Developer
ID, #606).
