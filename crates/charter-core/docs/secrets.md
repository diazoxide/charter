# Secrets and the vault directory

A **vault** is a named store of secrets, and charter hands a secret to a command without the
value ever passing through an agent's conversation. Every message an agent reads and every
tool result it gets can end up in a transcript — saved, logged, reviewed, or fed back into a
later prompt — so the transcript is not a safe place for a value, whatever holds it on disk.

## The commands

```bash
charter vault add devops --persona devops                          # a keyring vault, local by default
charter secret set devops API_TOKEN --stdin                         # value on stdin, never argv
charter secret list devops                                          # key names, never values
charter secret exec devops --env TOKEN=API_TOKEN -- curl -H "Authorization: Bearer $TOKEN" …
charter secret exec devops --file KUBECONFIG=PROD_KUBECONFIG -- kubectl get pods
charter persona secret exec --env TOKEN=API_TOKEN -- some-cli       # the active persona's vault
```

- **`secret exec <vault> -- <command>`** resolves each value inside charter and hands it to
  the command in its **environment** (`--env NAME=key`), in a temp file made **0600** whose
  path is the variable (`--file VAR=key`), or in a 0600 dotenv file (`--dotenv VAR=NAME:key`,
  repeats sharing a `VAR` merge into one file). The temp files are removed when the command
  ends, when charter fails, and when charter is stopped by any terminating signal it can
  catch; SIGKILL and a crash leave them in the temp directory. By default the command's output
  is captured and every value this call resolved is replaced by `***` before it is printed.
  That is a net against an accidental echo, not a boundary: a command that transforms a value
  (`base64`, a JSON re-encode) prints it unrecognised. `--stream` runs a long-lived command
  with its stdio attached and still removes its files; `--exec` replaces charter with the
  command and cannot take `--file` or `--dotenv`. Neither captures, so neither redacts.
- **`secret list <vault>`** prints the key names.
- **`secret get <vault> <key>`** prints a size band and a keyed fingerprint —
  `devops/API_TOKEN: present · 16–31 bytes · fp:9c41a0b7e5d2` — never the value. The
  fingerprint is `HMAC-SHA256` under a 32-byte key kept 0600 at `.charter/fingerprint.key`, so
  it compares values within this plane and cannot be checked against a guess without the key.
  `--reveal` prints the value only to a terminal, and refuses any other stdout unless `--force`.
- **`secret cp <vault> <key> <dest>`** writes the value to a new regular file at 0600 and prints
  only the path. A symlink, a device, a FIFO, a directory, an existing file (without `--force`)
  and charter's own stdin, stdout or stderr under any name are refused before the value is
  read. After that it is an ordinary file: no guard knows charter put a credential there.
- **`secret set`**, **`secret rm`** and **`secret audit`** (secrets older than `--days`, for a
  keyring or plain-file vault) write and inspect; `set` refuses an empty value unless `--allow-empty`.
- **`persona secret <verb> [--persona <name>]`** runs the same verb on the vault of the active
  persona: its `vault:` field, else the vault tagged with it. `vault: none` says the persona
  holds no credentials.
- **`vault add | list | verify | remove`** manage the registry. `vault list` shows each vault's
  provider, persona, scope and health, never a value; `vault verify` resolves every reference
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
  `cp` and `vault verify` read the item. On macOS an item is readable without a prompt only by
  the program that created it: any other program — `security find-generic-password -w`, a
  script, and also the other charter binary (the app and the `charter` command are two) — makes
  the Keychain ask you first, and "Always Allow" adds it. A charter update is a new binary, so
  with an ad-hoc signed build the first read after an update asks again.
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
- **`1password`** — charter keeps the vault in one 1Password item (`charter-<vault>`, or
  `--op-item`), each secret a concealed field of it, read and written through the `op` CLI; a
  value reaches `op` on stdin, never in its arguments.

A vault may declare the identity it is read through — `--env OP_SERVICE_ACCOUNT_TOKEN=<VAR>`
or `--token-env <VAR>` — as NAMES only. If `<VAR>` is unset, charter refuses rather than read
the vault as whoever the ambient token belongs to, and `secret exec` never hands one vault's
identity variables to a command run for another.

**The token itself belongs in the keyring, not in a shell.** Open the vault's tab in the app:
where its identity variable is set in the app's environment, the tab offers **Move this token
into the Keychain**. That stores the token in the system keyring — service `charter/identity`,
account the variable's name (`OP_TEAM_TOKEN`) — and marks the vault's identity as moved in
`.charter/vaults.json`, this machine's half of the registry. From then on every `charter secret`
command, in a chat or in a plain terminal, reads that variable from the keyring first and the
environment second, so a terminal that exports nothing still runs `charter secret exec`. The
mark is honoured only in this machine's half: a committed `vaults.json` cannot tell charter to
hand a keyring item to `op`. `vault list` and the tab say where each identity is from the mark,
without reading the keyring.

**No chat the app starts carries an `OP_*` variable** — not one the app inherited from the
shell that started it, and not one a harness profile's `env` declares. The extension programs
the app runs start from an empty environment and never had one.

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
  pseudo-terminal wrapper (`script`, `unbuffer`, a `pty` module) is a terminal to charter, and
  whatever it relays reaches the conversation. The Bash guard refuses the flag behind `script`
  and `unbuffer` when charter is named in the same command; it cannot see every way to make a
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
  paste the token straight into the keyring; it never touches charter's own environment. After
  that no chat the app starts is given the vault's identity variable (`OP_*` matched case
  insensitively, and every source name a vault declares, `VAULT_TOKEN` included), so
  `echo $OP_TEAM_TOKEN` there prints nothing. charter itself reads the token from the keyring for
  every `charter secret` a chat runs, so an agent can still use the vault — it cannot print the
  token.
- **A move from charter's own environment leaves the token in the app's process.** The tab also
  offers to move the token an app launched from an exporting shell already carries. That works,
  but a same-user process can read another's environment block (`ps -Eww`, `/proc/<pid>/environ`),
  so until you quit and relaunch charter from a shell that does not export it — and delete the
  export from your shell's startup files — a chat can still read it from charter. The tab warns
  while the app's environment still holds one. Pasting into the box avoids this; prefer it.
- **charter only runs the `op` it pinned when the token was stored.** A keyring-held identity is
  handed to the absolute `op` resolved from your PATH at store time, whose code-signing team is
  pinned too; a chat that drops its own `op` on `$PATH` cannot receive the token, and charter
  refuses rather than fall back to `$PATH`. Re-store the token if you move or reinstall `op`.
- **On macOS the two charter programs are asked for separately.** The token item is written by
  the app, so the first `charter secret` in a chat that reads it makes the Keychain ask whether
  `charter` may, and "Always Allow" adds it (ADR 0047). On Linux any process in your session can
  read an unlocked Secret Service collection.
- **Errors never repeat a stored entry.** A reference that does not resolve is named by its
  key, not by what the file holds under it, because that may be a value.
- **The app's Copy puts a value on the system clipboard for up to a minute.** While it is there,
  **any process running as you can read it** — that is what a clipboard is. charter marks the
  copy so clipboard-history apps skip it and clears it after 60 seconds, but only if the clipboard
  still holds that value; anything you copy in the meantime is left alone. The clear runs in the
  app, so **an app that crashes or is force-quit within the minute leaves the value on the
  clipboard** — it is cleared at a graceful quit, not a kill. And where Apple's **Universal
  Clipboard** is on, macOS may sync the copy to your other Apple devices, which charter cannot
  reach to clear; turn it off for a machine that copies secrets.

`charter doctor`'s vaults row does not check vaults yet.

## Where a vault lives

A plane keeps its state under `.charter/`, and `charter init` gitignores the whole of
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
  `sudo`, `charter secret exec … --`, `{ …; }`, `if …; then …; fi`), after a relocation however it is spelled (`cd`,
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
- **`--reveal`** on a charter invocation it can recognise.
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
- `charter save` refuses to commit a staged memory or ref file that matches.

A vault **reference** such as `token: "vault:forge/gh"` names where a secret lives and is not
a credential; both checks let it through.

Separately, a forge command that publishes prose (`gh issue create --body "…"`) and charter's
own text-taking commands (`persona remember`, `workspace remember|note|todo|vision`) refuse a
live command substitution in their text, because inside double quotes a backtick or `$(…)`
runs and can carry your whole environment into a public page. Write such text with
`--body-file -` and a **quoted** heredoc (`<<'BODY'`). [hooks.md](hooks.md) has the scope.
