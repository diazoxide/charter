# Secrets and the vault directory

A **vault** is a named store of secrets, and purlis hands a secret to a command without the
value ever passing through an agent's conversation. Every message an agent reads and every
tool result it gets can end up in a transcript — saved, logged, reviewed, or fed back into a
later prompt — so the transcript is not a safe place for a value, whatever holds it on disk.

## The commands

```bash
purlis vault add devops --persona devops                          # a keyring vault, local by default
purlis secret set devops API_TOKEN --stdin                         # value on stdin, never argv
purlis secret list devops                                          # key names, never values
purlis secret exec devops --env TOKEN=API_TOKEN -- curl -H "Authorization: Bearer $TOKEN" …
purlis secret exec devops --file KUBECONFIG=PROD_KUBECONFIG -- kubectl get pods
purlis persona secret exec --env TOKEN=API_TOKEN -- some-cli       # the active persona's vault
```

- **`secret exec <vault> -- <command>`** resolves each value inside purlis and hands it to
  the command in its **environment** (`--env NAME=key`), in a temp file made **0600** whose
  path is the variable (`--file VAR=key`), or in a 0600 dotenv file (`--dotenv VAR=NAME:key`,
  repeats sharing a `VAR` merge into one file). The temp files are removed when the command
  ends, when purlis fails, and when purlis is stopped by any terminating signal it can
  catch; SIGKILL and a crash leave them in the temp directory. By default the command's output
  is captured and every value this call resolved is replaced by `***` before it is printed.
  That is a net against an accidental echo, not a boundary: a command that transforms a value
  (`base64`, a JSON re-encode) prints it unrecognised. `--stream` runs a long-lived command
  with its stdio attached and still removes its files; `--exec` replaces purlis with the
  command and cannot take `--file` or `--dotenv`. Neither captures, so neither redacts.
- **In a sandboxed chat, the app runs it** (#1407). A sandboxed chat may not read any vault
  (ADR 0067 §5 class 1), so `secret exec` there reads nothing itself. It asks the app that
  started the chat, over the chat's hook channel, and the app reads that line only from a
  process inside that chat. One from a process outside it (tmux, `nohup` after its shell ended,
  `docker exec`) is refused with a sentence saying so, and nothing runs. The app checks that the vault registry tags the vault with the
  chat's persona: the persona the app started the chat as. A persona's own `vault:` line does
  not count, because a chat can edit that file. It then resolves the values and runs the
  command in a sandbox built from what the chat's own sandbox was compiled to when the chat
  started: the same denials (its harness's own among them), the chat's folder and a temp
  directory of its own as the only places it writes, and the network only through a proxy
  carrying the chat's hosts. A `--file` or `--dotenv` file is kept in the vaults folder, which
  every chat is denied, and only that command is given back a read of it. The command leads a
  process group of its own: everything it started that stayed in that group is killed when it
  ends or when the chat goes away. A process that leaves the group on purpose, such as a daemon,
  is not. A chat may
  have four such runs at once. Output streams back with each value's literal text masked, even
  under `--stream` and `--exec`, then the exit status. stdin is passed through when it is not a
  terminal.

  **Which vaults a chat may use.** One the vault registry tags for the persona the chat was
  opened as, or one you allowed for that persona on this machine. A vault that is neither is
  refused with a sentence naming the ways forward, and the chat's tab shows a notice with
  **Allow {persona} to use this vault** and **Keep blocked**. Allow is kept on this machine
  beside the project, never in the committed registry; it is recorded, listed in Settings ›
  Sandbox › Granted and revoked there, and the next run reads it, so the chat does not restart.
  A policy's `"vault-grants": false` forbids it. The other way forward is a dispatch to the
  persona the vault is tagged for. A chat's persona is fixed for its life, so nothing run in
  the chat changes which vaults it may use. A vault name the project does not register is
  refused as that.

  **What it keeps from the chat, and what it does not.** It keeps out the vault's storage and its
  provider's session, every vault the chat's persona may not use, and the credential file. It
  does not keep the values out: the command is the chat's own choice, so a chat can obtain any
  key of a vault its persona may use, for example by encoding it before printing. Masking
  matches a value's literal text only. Prefer `--file` to `--env` for a command that can read
  its credential from a file: an environment variable is readable by the same user through `ps
  eww` while the command runs. Linux has no such sandbox yet (#1040), so there the app refuses.
- **`secret list <vault>`** prints the key names.
- **`secret get <vault> <key>`** prints a size band and a keyed fingerprint —
  `devops/API_TOKEN: present · 16–31 bytes · fp:9c41a0b7e5d2` — never the value. The
  fingerprint is `HMAC-SHA256` under a 32-byte key kept 0600 at `.charter/fingerprint.key`, so
  it compares values within this plane and cannot be checked against a guess without the key.
  `--reveal` prints the value only to a terminal, and refuses any other stdout unless `--force`.
- **`secret cp <vault> <key> <dest>`** writes the value to a new regular file at 0600 and prints
  only the path. A symlink, a device, a FIFO, a directory, an existing file (without `--force`)
  and purlis's own stdin, stdout or stderr under any name are refused before the value is
  read. After that it is an ordinary file: no guard knows purlis put a credential there.
- **`secret set`**, **`secret rm`** and **`secret audit`** (secrets older than `--days`, for a
  keyring or plain-file vault) write and inspect; `set` refuses an empty value unless `--allow-empty`.
- **`persona secret <verb> [--persona <name>]`** runs the same verb on the vault of the active
  persona: its `vault:` field, else the vault tagged with it. `vault: none` says the persona
  holds no credentials.
- **`vault add | list | verify | remove`** manage the registry. `vault list` shows each vault's
  provider, persona, scope and health, never a value. In a sandboxed chat the app answers it,
  because the chat's sandbox denies it every provider's own files: each vault's provider and
  persona, and whether this chat may use it, with no scope or health column and no provider
  asked; `vault verify` resolves every reference
  for real and exits non-zero when one does not resolve.

`exec`, `cp` and `get --reveal` each record one event in the session trace naming the vault,
the keys and the command — never a value.

### Providers

- **`keyring`**, the default for `vault add` — the operating system's own credential store:
  the login Keychain on macOS, the Secret Service on Linux (ADR 0047). Each secret is one item,
  service `charter/<vault>/<8 hex>` (random per vault, made at its first write) and account
  `<key>`. A keyring cannot be asked what it holds, so the key names live in a keys index,
  `.charter/vaults/<vault>.keys.json` (0600, gitignored with the rest of `.charter/`), with each
  key's size band and when it was last written — never a value. `list`, `vault list` and
  `audit` read only the index, so they never make the Keychain ask you anything; `get`, `exec`,
  `cp` and `vault verify` read the item. On macOS every item is held to purlis's app: the app's
  own binary writes it (the `purlis` command hands the write to the app beside it), so only the
  app reads it without asking. Any other program — `security find-generic-password -w`, a
  script, a program a chat runs, and the `purlis` command too — makes the Keychain ask you
  first. Answer each ask as it comes. "Always Allow" for the `purlis` command lets any chat
  that runs the command read the item without asking; resolving secrets in purlis's own
  service will take the command out of the way (#1180). An item written before is written
  again, held, the next time purlis reads it (one the `purlis` command made, the next time
  the command reads it), and the vault's next read through the command says so once. A
  purlis update is a new binary, so with an ad-hoc signed build the first read after an
  update asks again.
- **`plain-file`** (`--provider plain-file`) — a JSON object of key → value at 0600,
  `.charter/vaults/<vault>.json` by default. It is **plaintext on disk**. Inside a plane that is a git repository, `vault add`
  refuses a `--file` git would commit, and `secret set` checks again before it writes, because
  the registry can be edited by hand or arrive in a commit. A file git already ignores, and
  one outside the plane, is accepted. `vault add` also refuses a file another registered vault
  already uses.
- **`reference`** — the file holds URIs, not values, resolved at read time:
  `op://<vault>/<item>/<field>` through `op read`, `vault://<path>#<FIELD>` through
  `vault kv get`. A reference file is safe to commit. `browser://` references are recognised
  and refused: the browser lane is not in this version yet.
- **`1password`** — purlis keeps the vault in one 1Password item (`charter-<vault>`, or
  `--op-item`), each secret a concealed field of it, read and written through the `op` CLI; a
  value reaches `op` on stdin, never in its arguments.

**Where purlis looks for `op` and `vault`.** In the `PATH` of the process that reads the vault,
then in the directories installers use under your home (`~/.local/bin` first), then in
Homebrew's and the system's. For a sandboxed chat that process is the app, so a program your
shell finds is found however the app was started, and nothing the chat sets changes where it is
looked for.

**A program where a chat may write is never run as a provider.** That is the project, any
folder you let chats write, the project's cache home, a harness's own folders, the temp
directories, and, for a read the app makes for one chat, that chat's own folder and what its
sandbox lets it write. The file is judged by where it really is, so a link to such a file is
refused too, and purlis runs the file itself, not the link. A copy further along the search
that no chat can write is used. With none, purlis refuses, names the file it passed over, and
stores no pin for it. The rule is the one a harness's program is held to.

A refusal names every directory searched. A program installed somewhere else is found once a
link to it is in `~/.local/bin`. `purlis doctor` has a row for each such program your vaults
use (`op for vaults`): green where it is found however purlis is started, a warning where only
this `PATH` finds it, where it is missing, and where the only one is where a chat may write.

A vault may declare the identity it is read through — `--env OP_SERVICE_ACCOUNT_TOKEN=<VAR>`
or `--token-env <VAR>` — as NAMES only. If `<VAR>` is unset, purlis refuses rather than read
the vault as whoever the ambient token belongs to, and `secret exec` never hands one vault's
identity variables to a command run for another. The refusal says the ways out in order: put
the token in the keyring from the vault's tab in the app, or export `<VAR>` where purlis runs.

**The token itself belongs in the keyring, not in a shell.** Open the vault's tab in the app.
It has a box to paste the token into, and it draws that box whether or not the vault could be
read: a vault whose token is nowhere shows the refusal and the box under it, and lists its
secrets by itself once the token is stored. This is the case of an app opened from the Dock,
which never sees what a shell exports. Where the identity variable is set in the app's
environment, the tab also offers **Move the token from purlis's environment**. Either way the
token goes into the system keyring, under an item of its own with a random name (service
`purlis/@identity/<id>`, account the variable's name, `OP_TEAM_TOKEN`), and the vault's
identity is marked as kept there in `.charter/vaults.json`, this machine's half of the
registry. From then on every `purlis secret` command, in a chat or in a plain terminal, reads
that variable from the keyring first and the environment second, so a terminal that exports
nothing still runs `purlis secret exec`. The mark is honoured only in this machine's half: a
committed `vaults.json` cannot tell purlis to hand a keyring item to `op`. `vault list`, the tab
and `purlis doctor` say where each identity is from the mark, without reading the keyring.

**A token is stored per vault: put it in from each vault's tab.** Storing a token marks the
one vault whose tab you are in, however many vaults are read through the same variable; no
vault is given a token you did not put in from its own tab. After a store the tab names the
other vaults read through that variable that still have none, each a link to its tab. Storing
a token again replaces it: the item the old one was kept under is deleted from the keyring
once the new one is in place.

**When a kept token does not read the vault**, the tab says the provider's own reason and
keeps the box. Most reasons are not the token's: a provider's program that is missing or has
moved (storing the token again pins the one found now), no network or a rate limit (read
again), and a refused sign-in, which is the one case the tab says to replace the token for.
A vault read through several variables has no box, since a box stores one token: start
purlis from a shell that exports them and move them from the tab.

**A chat starts from an allowlisted environment, not the app's whole one.** Whatever the app
inherited — from a terminal it was started in, `launchctl setenv`, a login item — reaches a chat
only if it is on the keep-list: what any program needs (`PATH`, `HOME`, `USER`, `LOGNAME`,
`SHELL`, `LANG`/`LC_*`, `TMPDIR`, `SSH_AUTH_SOCK`, `XDG_*`, the proxy variables, purlis's own
`CHARTER_*`), the variables the chat's harness declares for itself (`CLAUDE_*`, `CODEX_*`,
`OPENCODE_*`), and the names you add for the plane in `charter.local.toml`:

```toml
[chat_env]
pass = ["JAVA_HOME", "GO*"]   # a name, or a prefix ending in *
```

Cloud, forge and model-provider credentials (`GITHUB_TOKEN`, `GH_TOKEN`, `AWS_*`,
`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `NPM_TOKEN` and the like), and any name holding `KEY`,
`TOKEN`, `SECRET` or `PASSWORD`, are held back even when a built-in or harness prefix would admit
them. One passes only when you list its exact name there; a prefix you write does not bring a
credential with it. The table is read from `charter.local.toml` only, because a committed file
would let a teammate's push decide what of your machine's environment every chat gets.

Listing a credential there hands it to every chat in the plane, and so to the model's shell.
`purlis secret exec` is the way to give one command a credential.

**No chat the app starts carries an `OP_*` variable, or an identity variable a vault declares**
— not one the app inherited, not one a harness profile's `env` declares, and not one listed in
`[chat_env]`. The extension programs the app runs start from an empty environment too.

### Where the registry lives

`.charter/vaults.json` is this machine's half and `vaults.json` at the plane root is the
committed half; `vault add --share` writes the committed one. They are merged field by field,
this machine's winning, and a 1Password `--account` pin always stays local. A registration
holds names and paths, never a value.

A vault name is letters, digits, `.`, `_` and `-`, starts with a letter or digit, and never
holds `..`; a name that is not one is refused when it is registered and ignored when a
registry is read. A 1Password vault, item or account that starts with `-` is refused, since
`op` would read it as an option.

### The limits, said plainly

- **`--reveal` "to a terminal" means any terminal, including one the agent reads.** A
  pseudo-terminal wrapper (`script`, `unbuffer`, a `pty` module) is a terminal to purlis, and
  whatever it relays reaches the conversation. The Bash guard refuses the flag behind `script`
  and `unbuffer` when purlis is named in the same command; it cannot see every way to make a
  pty.
- **Output masking is best effort.** It replaces the exact text of each value this call
  resolved, and nothing else. A short value — a four-digit PIN, `true` — also matches ordinary
  output and is masked there, while the same value transformed by the command, split across
  writes that the command reorders, or printed in another encoding passes through.
- **A `--dotenv` file is written for dotenv parsers**, the `dotenv` package's rules. It is not
  shell syntax: do not `source` it, where a value's quoting would be read by the shell.
- **A persona is a label, not an access boundary.** `persona secret` picks a vault by the
  active persona's `vault:` field, but any chat can name any vault with `secret` directly, or
  pass `--persona`. Personas decide which vault is the default, not who may read it.
- **Put the token in the Keychain, do not leave it in a shell.** The vault's tab has a box to
  paste the token straight into the keyring; it never touches purlis's own environment. After
  that no chat the app starts is given the vault's identity variable (`OP_*` matched case
  insensitively, and every source name a vault declares, `VAULT_TOKEN` included), so
  `echo $OP_TEAM_TOKEN` there prints nothing. purlis itself reads the token from the keyring for
  every `purlis secret` a chat runs, so an agent can still use the vault — it cannot print the
  token.
- **A move from purlis's own environment leaves the token in the app's process.** The tab also
  offers to move the token an app launched from an exporting shell already carries. That works,
  but a same-user process can read another's environment block (`ps -Eww`, `/proc/<pid>/environ`),
  so until you quit and relaunch purlis from a shell that does not export it — and delete the
  export from your shell's startup files — a chat can still read it from purlis. The tab warns
  while the app's environment still holds one. Pasting into the box avoids this; prefer it.
- **purlis only runs the `op` it pinned when the token was stored.** A keyring-held identity is
  handed to the absolute `op` purlis found at store time, whose code-signing team is
  pinned too; a chat that drops its own `op` on `$PATH` cannot receive the token, and purlis
  refuses rather than fall back to `$PATH`. Re-store the token if you move or reinstall `op`.
- **On macOS the two purlis programs are asked for separately.** The token item is written by
  the app, so the first `purlis secret` in a chat that reads it makes the Keychain ask whether
  `purlis` may, and "Always Allow" adds it (ADR 0047). On Linux any process in your session can
  read an unlocked Secret Service collection.
- **Errors never repeat a stored entry.** A reference that does not resolve is named by its
  key, not by what the file holds under it, because that may be a value.
- **The app's Copy puts a value on the system clipboard for up to a minute.** While it is there,
  **any process running as you can read it** — that is what a clipboard is. purlis marks the
  copy so clipboard-history apps skip it and clears it after 60 seconds, but only if the clipboard
  still holds that value; anything you copy in the meantime is left alone. The clear runs in the
  app, so **an app that crashes or is force-quit within the minute leaves the value on the
  clipboard** — it is cleared at a graceful quit, not a kill. And where Apple's **Universal
  Clipboard** is on, macOS may sync the copy to your other Apple devices, which purlis cannot
  reach to clear; turn it off for a machine that copies secrets.

`purlis doctor`'s vaults row does not check vaults yet. Its `op for vaults` and `vault for
vaults` rows say only whether each provider's program is found. Its `vault tokens` row says,
for each vault read through an identity variable, whether the token is marked as kept in the
system keyring, in this environment only, or nowhere. It warns for the last two. It says so
from this machine's record and reads no keyring item, so "marked as kept" is not a read that
succeeded. Run inside a chat it prints no such row: a chat is given no identity variable, so
from there an exported token cannot be told from a missing one.

## Where a vault lives

A plane keeps its state under `.charter/`, and `purlis init` gitignores the whole of
`/.charter/`, which is what keeps a vault out of a commit. Inside it the guards treat these
entries as secret, because their **content** is the secret:

- `.charter/vaults/` — the vault directory and every file under it;
- `.charter/browser…`, `.charter/active-…` and `.charter/fingerprint…` — a browser profile,
  the active-persona marker and the fingerprint key;
- `.charter/` itself, named as a whole.

`.charter/vaults.json` — the registry, which holds provider settings and file paths, never a
value — is an ordinary file and is not refused.

A file in `.charter/vaults/` is plaintext on disk. The guards below are about the
conversation, not about encryption at rest: anyone who can read your account's files can
read that directory.

## What the guards refuse

All of these answer in `charter hook pretooluse`, which the app's plugin runs before each
tool call. [hooks.md](hooks.md) has the whole list and its limits.

- **A reader pointed at a vault path.** `cat`, `less`, `more`, `head`, `tail`, `bat`, `nl`,
  `tac`, `xxd`, `od`, `strings`, `grep`, `rg`, `ag`, `awk` and `sed`, with a guarded path
  as an operand or an input redirection (`< <vault> tee`), behind any wrapper (`env`,
  `sudo`, `purlis secret exec … --`, `{ …; }`, `if …; then …; fi`), after a relocation however it is spelled (`cd`,
  `pushd`, `env -C`, `sudo --chdir`), and on any line of a multi-line command. A wrapper that
  opens a file itself (`xargs -a <vault>`) is a read of that file.
- **The harness's own file tools.** `Read` or `Grep` on a guarded path is refused on the same
  predicate as the shell route — the two do not differ on any spelling.
- **A walk that reaches the vault directory.** `grep -rn TOKEN .` from the plane root names
  no vault file and prints every one of them. The guard resolves the operand against the
  shell's directory and asks whether the walk would reach `.charter/`'s guarded entries; it
  fires only when they exist and hold something. The denial names the fix —
  `grep -rn --exclude-dir=.charter …` or `rg --glob '!.charter' …`. A `Grep` with no path
  walks the directory it stands in and is judged the same way.
- **`--reveal`** on a purlis invocation it can recognise.
- **A write into `.charter/`** with the `Write` or `Edit` tool, inside a plane — that
  directory decides which commands run without a prompt.

Path spellings are folded before they are compared: `.charter//vaults/`,
`.charter/./vaults/` and `.CHARTER/vaults/` are the same file to the guard, as they are to
the filesystem on macOS. `\` is not folded, because on POSIX it is an ordinary filename
character.

**A denial is the guard working, not a bug.** Every denial says so, and says there is no
switch that lifts it; see [hooks.md](hooks.md) → *When a guard is wrong*.

## Where the guard stops

It matches a **known program name** against a **path spelled in the command line, before
any shell runs**. Everything outside that sentence gets through, and it is stated here so
you do not have to discover it:

- *The name.* A program that is not on the reader list runs: an interpreter
  (`python3 -c "print(open('.charter/vaults/db.json').read())"`), `base64`, `cp`, `jq`,
  `cut`, `dd`, `git show`, and a shell string (`sh -c 'cat …'`), which reaches the guard as
  one opaque argument.
- *The path.* A different path holding the same bytes — a symlink, a copy, a vault file kept
  outside `.charter/` — is an ordinary file to every guard.
- *The walk.* Only programs known to walk directories are asked where they go, so
  `find . -type f -exec cat {} +`, `tar cf - .` and an interpreter reach the same files.
- *The shell.* Every expansion is a read the guard does not see: a glob inside the vault
  path (`cat .charter/vault?/db.json`), a variable (`V=…; cat $V`), a quoted
  `"$(cat …)"`, brace or tilde expansion. A glob only escapes when it falls inside the
  guarded part of the path, so `cat .charter/vaults/*.json` is still refused.

It is a guard against mistakes, not against someone deliberately spelling around it.

## A credential in committed text

A plane commits and pushes its memory, so a secret written into one is disclosed the moment
it lands. Two checks look for a credential's **shape** — a JWT, an AWS access key, a
`token: <value>` assignment — and name the kind, never the matched text:

- after a memory or ref file is written, the chat is told to remove it;
- `purlis save` refuses to commit a staged memory or ref file that matches.

A vault **reference** such as `token: "vault:forge/gh"` names where a secret lives and is not
a credential; both checks let it through.

Separately, a forge command that publishes prose (`gh issue create --body "…"`) and purlis's
own text-taking commands (`persona remember`, `workspace remember|note|todo|vision`) refuse a
live command substitution in their text, because inside double quotes a backtick or `$(…)`
runs and can carry your whole environment into a public page. A process substitution (`<(…)`,
`>(…)`, zsh's `=(…)`) is refused on those lines too. Write such text with
`--body-file -` and a **quoted** heredoc (`<<'BODY'`). [hooks.md](hooks.md) has the scope.
