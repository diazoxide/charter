# Secrets: the vault

`charter` has a small, provider-agnostic secret manager: **vaults**, addressed via
`charter vault …` and `charter secret …` (or, scoped to a role, `charter persona
secret …`). Read this page before storing anything real in one.

## What it actually is — plainly

**The default vault is not a password manager, and it is not a secrets manager in the
sense 1Password or Vault are.** The `plain-file` provider stores secrets as **plaintext
JSON on disk**, at file mode **0600** (owner
read/write only). There is **no encryption at rest**. Anyone with read access to your
user account, or a backup of your home directory, or a malicious process running as
you, can read the file directly. `charter` does not pretend otherwise: the vault
registry and every vault file live under `.charter/` (gitignored — never committed, never
synced anywhere by `charter` itself).

Every directory **the vault writers create** on the way to a vault file is 0700, each
level of it and not just the last, so the directory listing your vault *names* is not
readable by other accounts either. A directory that **already existed** keeps the mode it
has — a `.charter/vaults/` made by hand or by an older charter is 0755 before `secret set`
and 0755 after. charter will not chmod a directory it did not create (a vault's `--file`
can name any path on this machine, and silently tightening someone's home or a shared team
directory is worse than the thing it fixes), so it reports it instead, on the vault's
health line — which is the `STATUS` column of `charter vault list`:

```
devops: 3 secret(s), listed by other accounts: .charter/vaults 755 (want 700 — chmod 700)
```

One `chmod 700 .charter/vaults` clears it.

**Two limits on that sentence, both measured rather than assumed.**

`.charter/` itself is usually *not* one of the directories the vault writers create. On
the default flow, `charter vault add` writes the local registry before any vault file
exists, and that write creates `.charter/` with no mode of its own — so under `umask 022`
the state directory comes out 0755 and stays there, and under `umask 077` it comes out
0700. The umask decides that one level; charter decides every level below it. The report
above still names it (`listed by other accounts: .charter 755`) and one `chmod 700
.charter` clears it — but the paragraph above is a claim about the vault writers, and the
state directory is not theirs to create — [#470](https://github.com/diazoxide/charter/issues/470).

And the report reaches `charter vault list` only. `charter doctor` asks each vault
whether it is *reachable* and discards the rest of the health line, so a loose directory
shows up in neither `doctor` nor `doctor --json` — [#471](https://github.com/diazoxide/charter/issues/471).

If you want encryption at rest, use your **OS keychain** (macOS Keychain, a real
password manager, or a proper secrets backend) to hold the credential, and treat
`charter`'s vault as a *thin, disposable staging area* your agent reads from — or wait
for a keychain-backed provider (the `VaultProvider` interface is designed so one can be
added without touching call sites; none ships yet).

## What it genuinely does

What the vault protects against is a different, narrower, and very real threat: an
**AI agent's own conversation** is not a safe place for a credential. Every message an
agent reads, every tool result, every line it prints, can end up in a **transcript** —
saved, logged, reviewed, or (worse) fed back into a later prompt. The vault's actual job
is keeping a secret value **out of the model's context, the terminal transcript, and
shell history**, while still letting an agent *use* the credential:

- **`charter secret exec`** runs a command with secrets injected as environment
  variables or temp files that the agent names only by *key*, never by value, and
  **redacts** every occurrence of the resolved value from the command's captured
  output before anything is printed:

  ```
  charter secret exec devops --env TOKEN=API_TOKEN -- curl -H "Authorization: Bearer $TOKEN" https://…
  ```

  Redaction is a **net against an accidental echo, not a boundary** — it is a literal
  search for the value's own bytes, so a command that *transforms* the value comes back
  unscrubbed, and `--exec`/`--stream` capture nothing and therefore redact nothing. The
  guarantee is that charter never prints the value into the conversation; where it goes
  after that is a property of the command you chose. Details below, and in
  [SECURITY.md](../SECURITY.md).

- **`charter secret cp`** materializes a secret to a 0600 file (e.g. a kubeconfig) and
  prints only the path, never the contents. The destination has to be a **real file it
  creates**: a device, a FIFO, a directory or a symlink is refused, because
  `/dev/stdout`, `/dev/stderr` and `/dev/fd/*` are the agent's own transcript and
  writing there is the leak this command exists to avoid. A destination that turns out
  to **be** one of charter's own streams is refused whatever it is called — the test is
  `(st_dev, st_ino)` from an `fstat` of the descriptor charter opened, compared against
  its own stdin, stdout and stderr, so a hardlink, a `readlink`'d log path and
  `/dev/fd/1` are all the same one object and get the same answer. `--force` does not
  reach that check. An **existing** file is refused too — overwriting one destroys its
  contents and sets it to 0600, so it takes `--force` and says so afterwards.

  What it cannot do is follow the file afterwards. Once written, that path is an ordinary
  file: `cat`-ing it is not denied, because no guard knows charter put a credential there.
  `cp` is for handing a **path** to a tool that needs one — a kubeconfig, a PEM — not for
  getting at the value. Delete it when the tool is done; `secret exec --file` does that for
  you and is the better shape whenever the tool's lifetime is one command.
- **`charter secret get`** is masked by default — it prints a size band and a keyed
  fingerprint, never the value:

  ```
  devops/API_TOKEN: present · 32–63 bytes · fp:9c41a0b7e5d2
  ```

  The fingerprint is `HMAC-SHA256(plane key, value)`, not a hash of the value, and the
  plane key is 32 random bytes generated on first use and kept 0600 in `.charter/`. That
  matters because this line travels: into an agent's transcript, a pasted terminal, a
  ticket. An unkeyed digest plus an exact byte count — what charter printed before
  0.51.x — is checkable **offline**, so a wordlist run against that line confirms a
  guessed password with no further access to charter at all. Keyed, it is only comparable
  to another fingerprint printed by this same plane, which is the only comparison anyone
  actually makes: *is this still the value I set*, *does that vault hold the same one*.
  The same value fingerprints differently on a different machine, deliberately. The size
  is banded for the same reason — an exact length prefilters a wordlist; `32–63 bytes`
  barely does.

  If `.charter/` is read-only and no key can be made, the fingerprint is **omitted** —
  never replaced by an unkeyed one.

  The key is the whole strength of this, so `.charter/fingerprint.key` is denied to
  file-reading tools on the same terms as `.charter/vaults/` — which matters most for a
  1Password-backed vault, where there is no vault file on the machine and the key would
  be the only readable thing between the printed line and the value. A shell running as
  you reads it, as it reads everything else you own; this is a guard rail, not a
  guarantee. And within one plane the fingerprint remains an equality oracle: anyone who
  can `charter secret set` a guess can compare. They can also `--reveal --force`, so it
  is not a step up for them — it is the reason the key is per-plane rather than
  per-vault, which is what keeps "do these two vaults hold the same value" answerable.
- **`charter secret get --reveal`** is the one path that *can* print plaintext, and it
  deliberately refuses to do so to a **non-interactive stdout** (the exact channel
  through which a value would leak straight into an agent's context) unless you pass
  `--force` — it's meant for a human at a real terminal, not a script or an agent.
- Values are always **written** via `--stdin` or `--from-file`, never as a bare CLI
  argument — an argument shows up in shell history and `ps` output for any other
  process on the machine to read.
- A Claude Code guard hook denies `--reveal` on a charter invocation it can recognise, and
  denies known reader programs pointed at a vault file — both would print a secret straight
  into the conversation. **A denial here is that guard working, not a bug** — see the
  README's "one credential" section for the same idea applied to git auth.

  "Known reader programs" covers the shell (`cat`, `grep`, `head`, … on
  `.charter/vaults/…`) **and** the harness's own file-reading tools (`Read`, `Grep`). It
  used to mean only the shell, which made this bullet false in the way that mattered: the
  shell denial names the path it refused, so reading that path with `Read` was the obvious
  next move and it worked (#90). It is a check on the names in the argv the hook can see, so
  a reader that is not on the list, or one reached by a route the hook cannot read, is not
  covered — the list closes the accidental paths, not a chosen one.

  It also covers the ways of spelling the same thing: `.charter//vaults/`, `.charter/./vaults/`
  and `.CHARTER/vaults/` are the same file to the filesystem and are the same file to the
  guard; a wrapper in front of the reader (`env cat …`, `sudo cat …`, `{ cat …; }`,
  `if true; then cat …; fi`) does not change what the program is, and a wrapper that opens
  a file itself (`xargs -a …`) is a read of that file; a redirection is not the program and
  not an operand, so `< <vault> cat` and `tee < <vault>` are both reads of the vault; a
  command boundary is an operator the shell would *interpret*, so a quoted or escaped one
  (`cat \) …`, `cat '(' …`, `cat '&&' …`) is an argument to the reader rather than a
  boundary and the `&` in a `2>&1` belongs to the redirection, while an interpreted newline
  is a boundary like `;` and `#` opens a comment only at a word start; and a relocation
  counts however it is spelled, so `cd .charter/vaults && cat db.json`, `pushd`, `env -C`
  and `sudo --chdir` all land where they point. Each of those was a verified bypass, and
  each is now a test. What it does **not** cover is written down in
  [hooks.md](hooks.md#where-the-secret-leak-guard-stops) — a quoted `"$(cat …)"`, a glob or
  brace spelling of the path, `sh -c`, and a file `charter secret cp` wrote to a path you
  chose. It is a guard against mistakes, not against someone
  deliberately spelling around it; for values that warrant real custody the provider, not
  the hook, is the control.

  `Glob` is not denied — it returns file *names*, and that a vault exists is not the secret.
  A **recursive search rooted above the vault directory is** — `grep -rn TOKEN .` from the
  plane root reads every vault file as collateral, and since #474 both routes refuse it and
  name the exclusion that fixes it. What the two routes do **not** differ on is any spelling
  of a guarded path: they call one
  predicate on the operand as written and neither adds a step of its own, which is asserted
  in both directions rather than assumed — the one round where the read route carried an
  extra step, the Bash route allowed `grep -rn TOKEN .charter/vaults` while `Grep` on the
  same directory was refused.

  **What the guard does not catch, stated so you do not have to discover it.** The whole of
  it is one sentence: **the guard matches a known program NAME against a path SPELLED IN THE
  COMMAND LINE, before any shell runs.** Four things fall out of that sentence. It is a
  claim about *that sentence's consequences*, not a promise that the list is exhaustive —
  the review that produced this section found a fifth by re-reading the code, not the prose,
  and the honest version of the promise is that each item below is pinned as behaviour in
  `tests/test_documented_limits.py` or `tests/test_vault_path_spellings.py`.

  *The name.* Everything not on the reader list runs: an interpreter
  (`python3 -c "print(open('.charter/vaults/db.json').read())"`), a program that reads
  without being called a reader (`base64`, `cp`, `jq`, `cut`, `dd`,
  `git show HEAD:.charter/vaults/db.json`), and a shell string (`sh -c 'cat …'`), which
  reaches the guard as a single opaque argument and is not re-parsed. Adding names does not
  close this: the missing one is always the next one, and a longer list starts denying
  ordinary work.

  *The path spelling.* Redundant `/` separators, `.`/`..` segments and letter case are
  normalised, so `.charter//vaults/db.json`, `.charter/./vaults/db.json` and
  `.CHARTER/vaults/db.json` answer the same as the plain form — and so does the directory
  itself, `.charter/vaults`, with or without the trailing slash. Two things are left. A
  *different* path holding the same bytes: a vault registered outside `.charter/` (see
  below), a file `charter secret cp` materialised at a path you chose, or a symlink — each
  an ordinary file to every guard charter has. And a separator that is not `/`:
  normalisation is POSIX, so a Windows-style `.charter\vaults\db.json` is not folded,
  because on POSIX a backslash is an ordinary filename character and folding it would deny
  real filenames. charter's harness targets POSIX — macOS and Linux, with tmux — and is
  neither tested nor supported on Windows, which is why the guard is not made to answer
  differently there (#476).

  *The path you actually named — and, since #474, the path the walk reaches.* An operand
  that **contains** the vault directory without naming it used to be allowed: `grep -rn
  TOKEN .` from the plane root reads every vault file as collateral and names none of them.
  A second predicate now covers it on both routes. It asks whether the walk would **reach**
  charter's state directory rather than how the operand is spelled — the operand is resolved
  against the shell's directory and compared by ancestry, so `.`, `..`, `../..`, an absolute
  path and a symlinked parent are one question — and it fires only when the guarded entries
  exist and hold something, so a plane with no file-backed vault never sees it. The denial
  names the fix: `grep -rn --exclude-dir=.charter …`, `rg --glob '!.charter' …`, and both
  are asserted to run in `tests/test_vault_path_spellings.py`.

  The ceiling on *that* predicate is the same one the reader list has: it knows which
  programs walk directories, so `find . -type f -exec cat {} +`, `tar cf - .` and an
  interpreter all reach the same files unguarded. `charter init` gitignores the whole of
  `/.charter/`, which is what actually keeps a vault out of a commit.

  *Before any shell runs.* This is the one people discover the hard way. The hook is handed
  the command line and never sees what the shell makes of it, so every expansion is a read
  the guard did not see: a glob (`cat .charter/vault?/db.json`,
  `head -c 400 .charter/vault*/db.json`, `cat .cha*ter/vaults/db.json`), a variable
  (`V=.charter/vaults/db.json; cat $V`), a command substitution, brace or tilde expansion,
  and a changed working directory (`cd .charter/vaults && cat db.json`). Every one of those
  is `cat` on the same inode as the denied form. They are not separate holes to close one at
  a time — they are one fact with as many spellings as the shell has constructs, and a guard
  that started expanding them would be a shell with a shell's bugs. One edge is worth
  knowing: a glob only escapes when the metacharacter falls *inside* `.charter/vaults/`, so
  `cat .charter/vaults/*.json` is still denied.

  Treat all of it the way [SECURITY.md](../SECURITY.md) frames it: the guard is against
  mistakes, and the property that does not depend on a name is that *charter* never prints
  the value.

## Setting one up

```
charter vault add devops --provider plain-file --file .charter/vaults/devops.json --persona devops
charter secret set devops API_TOKEN --stdin
charter secret list devops                 # keys only, never values
charter secret audit devops --days 90       # flag anything old enough to rotate
```

Or via a persona (`charter persona create --with-vault` already does the `vault add`
step for you): once a vault is tagged with `--persona <name>`, `charter persona
secret …` resolves it automatically — no vault name needed on every call:

```
charter vault add devops --provider plain-file --persona devops
charter persona secret set API_TOKEN --stdin        # resolves the active persona's vault
charter persona secret exec --env TOKEN=API_TOKEN -- some-cli
```

### Binding the identity a vault is read through

`op` authenticates from a single global `OP_SERVICE_ACCOUNT_TOKEN`, but least-privilege
setups issue **one service account per scope**, so the tokens are named per persona. A
vault can declare which one it is read through:

```
charter vault add devops --provider reference --file secrets/devops.json \
  --token-env OP_ACME_DEVOPS_SERVICE_ACCOUNT_TOKEN --share
```

charter sets `OP_SERVICE_ACCOUNT_TOKEN` from that variable for the duration of that
vault's `op` call, and for nothing else. Only **names** are stored — never a value — so
the registry stays inert if it leaks and the binding is reviewable in git:

```json
"config": { "env": { "OP_SERVICE_ACCOUNT_TOKEN": "OP_ACME_DEVOPS_SERVICE_ACCOUNT_TOKEN" } }
```

`--token-env` is shorthand. The general form names both sides, because reference vaults
resolve `vault://` as well as `op://` and HashiCorp reads a different variable entirely:

```
charter vault add infra --provider reference --env VAULT_TOKEN=ACME_VAULT_TOKEN
```

**A declared variable that is unset is an error, not a fallback.** charter will not reach
for an ambient `OP_SERVICE_ACCOUNT_TOKEN`: that reads the vault under an identity it never
declared, and 1Password answers with "no items" or a permission error — so the failure
arrives disguised as an empty vault rather than as the wrong credential. Vaults that
declare nothing are untouched, so single-account setups see no change.

`charter vault list` and `charter doctor` report a missing identity as its own state,
separate from an unhealthy vault, because the fix is an `export` rather than anything
about the vault:

```
→ identity variable unset for: devops — export $OP_ACME_DEVOPS_SERVICE_ACCOUNT_TOKEN
  (charter will not fall back to an ambient token; that would read the vault as someone else)
```

A read that fails anyway names the identity in play, so a permission error points at the
credential rather than reading as an empty vault.

**A declared identity does not travel to someone else's child.** `charter secret exec`
runs the command with this shell's environment **minus every identity variable declared
by a vault other than the one being read** — both halves of each binding, the variable
the CLI reads and the variable this machine carries it in. Without that, one binding is
least-privilege and the process it starts is not: `charter secret exec qa -- <cmd>` used
to hand `<cmd>` the devops *and* marketing *and* personal service-account tokens, in the
clear, while redacting the single value the model actually named. The vault being read
keeps its own names, so `charter secret exec devops -- charter secret get devops K`
still works, and a vault that declares no identity is untouched.

### Where a registration is recorded: two files, one view

| File | Committed? | Holds |
| --- | --- | --- |
| `vaults.json` (plane root) | **yes** | what is the same on every machine: provider, persona, `op-vault`, and a `file` relative to the plane |
| `.charter/vaults.json` | no (0600) | this developer's own vaults, plus per-machine overrides — which wins on conflict |

Reads see them merged, **field by field**, so you can pin an `account` on a vault the team
declares without restating its provider and file (restating them is how a local copy
silently drifts from the shared one).

`charter vault add` writes the **local** file unless you pass `--share`:

```
charter vault add team --provider reference --file secrets/team.json --share
```

Local-by-default is deliberate, and it is the same posture as `[memory].share` — which
defaults to `local` so a control plane never publishes by accident. It matters more here:
a registry names which personas hold credentials and where their files live, which is a
useful map even without the values.

**A shared `file` may be absolute, and that is on purpose** — a team that provisions the
file out of band wants the pointer to travel, and pointing `--file` outside the plane is
what charter tells you to do when a plain-file vault would otherwise land somewhere git
tracks. The consequence is worth stating plainly: the committed half can name any path on
the machine as a vault. Nothing unattended reads or writes it — `charter doctor` and the
status line ask a vault whether it is reachable and no longer touch it — but `charter
secret get`/`set` against that vault name would. `doctor` names a shared vault whose file
lands outside the plane on its vaults line, so the configuration is visible rather than
merely legal.

**A vault file outside `.charter/` is also outside the guard.** The leak guard and the
`Read`/`Grep` guard both recognise a vault by its *path* — `.charter/vaults/…` — so a
plain-file vault at `~/creds/devops.json` is an ordinary file to them, and `cat` on it is
an ordinary read. That is the direct cost of the remedy in the paragraph above, and it is
not hidden in the code: `charter/hooks.py` says so where the check is made, and explains
why it is not fixed there — this runs on **every** Bash tool call, and consulting the vault
registry per invocation is a real cost on a hot path. Prefer the default location under
`.charter/`, which `charter init` gitignores; move the file out only when the alternative
is committing plaintext, and know which property you traded for which.

`--account` never travels, even with `--share`. It is the one field that genuinely differs
per machine, so it is split off and written locally.

This is what makes committed vault files usable. A **reference** vault holds `op://` URIs
rather than values, so teams commit them — but before this the *index* that located them
recorded one developer's home directory, so a fresh clone found the vault files present
and unreachable, and had to re-register them by hand. Now `git clone` is enough:

```
$ charter vault list          # on a machine with no .charter/ at all
VAULT     PROVIDER    PERSONA  SCOPE   STATUS
team      reference   —        shared  1 reference(s) via op
```

The `SCOPE` column answers the two questions a two-file registry invites: *why can't my
teammate see this vault*, and *why is this vault in git*.

### Registering over a name that already exists

`charter vault add` **refuses** a name that is already registered, naming the provider in
the way and where its secrets live:

```
✗ vault 'devops' is already registered with provider 'plain-file' (.charter/vaults/devops.json).
  charter will not replace it: the registration is the only pointer to that vault's
  secrets, so replacing it strands them with nothing referring to them.
```

That pointer is the whole reason. A plain-file vault's secrets are found *only* through
its registration, so overwriting it does not move anything — the file stays on disk with
nothing referring to it, and `charter secret get` then reports the key as missing rather
than as unreachable. Same additive rule `charter init` and `reinit` follow: name the
blocker, refuse, never delete or rename to make room.

`--force` overrides it, warns, and tells you where the old vault's file remains. It does
**not** migrate secrets — moving them between providers is a deliberate operation, not
something that rides along inside `add`.

## Provider status

| Provider | id | Status |
| --- | --- | --- |
| Plain file (JSON, 0600) | `plain-file` | Implemented. Stores the value itself. |
| Reference (`op://`, `vault://`) | `reference` | Implemented. Stores a **URI**; the value is fetched at read time. |
| 1Password (native) | `1password` | Implemented. One item per vault; charter **creates and manages** its fields via the `op` CLI. |

If 1Password is where your credentials belong, you have two shapes to choose between,
and the difference is who owns the item:

- **`1password`** — charter owns it. `secret set` creates the item and writes the field,
  `rm` removes the field, so a credential can be provisioned for a persona without
  opening the 1Password UI. The item itself is the vault and charter never deletes it.
- **`reference`** — someone else owns it. charter stores only a pointer to an item that
  already exists. Right when the credential is shared with people or systems beyond
  charter, or when a human should stay in charge of rotating it.

### Native 1Password vaults

A `1password` vault is **one 1Password item whose custom fields are the secrets**:

```
charter vault 'devops', key 'AWS_ACCESS_KEY_ID'
    → item  charter-devops   (override with --op-item)
      tagged charter, charter:devops
      field AWS_ACCESS_KEY_ID, concealed
```

One item means one ACL to manage and one thing to find in the 1Password UI, and it lets
charter describe layouts it previously could not — two keys sharing an item, or a value in
`notesPlain` rather than `password`. Point `--op-item` at an item you already curate to
adopt it as-is; no vault file is created either way.

Reads fetch one field (`op read op://<vault>/<item>/<field>`). Writes are a read-modify-write
of the whole item, because `op` templates replace rather than merge — so `set` re-reads the
field afterwards and fails loudly if it did not land, which is what a concurrent writer
looks like. 1Password keeps item history if that ever happens.

```bash
charter vault add devops --provider 1password --op-vault Engineering --persona devops
charter secret set devops KUBECONFIG --from-file ~/.kube/prod.yaml
charter secret exec devops --file KUBECONFIG=KUBECONFIG -- kubectl get pods
```

**One item per key came first, and was replaced.** It gave each secret its own item,
`charter-<vault>-<KEY>`, with the value in that item's `password` field. It could describe
nothing but that shape, so anyone whose 1Password already looked different kept a separate
file of `op://` URIs alongside their vault to work around it.

It was chosen to avoid one specific hazard, and that hazard is real: `op item get
--format json` **conceals** values unless asked otherwise, so a naive read-modify-write
of a multi-field item would write masks back over every sibling secret. The answer is
`--reveal`. That flag is why the write path survives the round-trip, and why it exists at
all — `set` and `rm` pass it, and nothing else does, because nothing else has any business
pulling a whole vault's values into memory.

If you registered a vault under the old schema, its `charter-<vault>-<KEY>` items are
still in 1Password and charter no longer reads them. It will not call such a vault
healthy-and-empty either: `charter vault list` counts the leftovers and says what to do
with them, and `charter doctor` marks the vault unhealthy rather than fine. Re-register
each credential with `charter secret set`, then delete the old items.

Deliberate properties:

- **No secret ever reaches argv.** 1Password's own help warns that "command arguments
  get logged in your command history, and can be visible to other processes on your
  machine". Writes pipe a JSON template on **stdin** (`op item create -`,
  `op item edit <item>`); only names are ever passed as arguments.
- **charter looks at one item and no others.** `secret list` reads the fields of this
  vault's item, so a shared 1Password vault your team also fills by hand shows you
  nothing but the item you pointed charter at. The `charter:<vault>` tag marks what
  charter wrote; it is how `vault list` recognises leftovers from the old schema, not a
  filter the listing runs through.
- **Errors withhold `op`'s output.** Its stderr can echo what it was given, and on a
  read path its stdout *is* the secret; failures report the exit status only.
- **Pin the account** with `--account` when signed into more than one. Otherwise an
  unqualified vault name resolves against whichever is default — a quiet way to write
  a credential into the wrong company's vault.
- **`charter doctor` and `vault list` never read a value.** They count items, so
  routine status never triggers a 1Password re-auth prompt.

**A service-account token needs WRITE access to the vault.** A token that can read it
still fails every write with 1Password error `(101) You do not have permission to
perform this action` — reads, `secret list` and `doctor` all look healthy while
`secret set` refuses. charter's write errors name this first, because the obvious
suspects (signed in? vault exists?) are both fine when it happens.

### Reference vaults — when your secrets already live somewhere else

If your team already runs HashiCorp Vault or 1Password, a charter vault would be a
**third** place a credential lives, and a third place it can go stale. A reference
vault avoids that: the file holds a pointer, not a secret.

```bash
charter vault add team --provider reference --file .charter/vaults/team.json
charter secret set team DEPLOY_TOKEN --value 'op://Eng/deploy/token'
```

`secret set` on a reference vault takes a **URI**, never a value — accepting one would
turn it into a plaintext vault without saying so. If what you have is the secret itself
and you want charter to store it, you want the other shape: a `1password` vault owns its
items and `secret set` creates them, with the value arriving on stdin and never in argv.

```bash
charter vault add team-owned --provider 1password --op-vault Engineering
… | charter secret set team-owned DEPLOY_TOKEN
```

The distinction is who owns the item, not which CLI is involved — both end up in
1Password. Reach for `reference` when a human or another system should stay in charge of
rotating the credential, and for `1password` when charter should.

#### `browser://` — the token a logged-in session is holding

A browser session is a place a credential lives, so it is a reference like any other. This
closes the step after a bridged login: check that the UI and the API agree, by calling the
API **as the user you just logged in**.

```bash
charter secret set qa API_TOKEN --value 'browser://owner/localstorage/access_token'
charter secret exec qa --env TOKEN=API_TOKEN -- \
  curl -sH "Authorization: Bearer $TOKEN" https://api.example.test/me
```

charter never puts the value in your transcript on this path, and prints it only where you
asked for it yourself — `secret get --reveal` to your terminal, or `secret cp <dest>` into a
real file you named. That matters more
here than it looks, because the obvious alternative is the idiom Playwright's own reference
documents:

```bash
TOKEN=$(playwright-cli --raw cookie-get session_id)   # ← the leak
```

Command substitution puts the token in a shell variable, in a transcript, with nothing
redacting it — the outcome the whole browser lane exists to prevent. Through a reference it
is resolved at the moment of use, injected into the child, and scrubbed from the output that
comes back.

Two things charter needs to be right about, and one it will not do:

- **The session must be open**, and `charter secret exec` does not open it. Read the token
  inside the same flow that logged in; a resolve against a closed session fails with the
  vendor's `not open`.
- **The version is charter's pin** unless the vault overrides it with
  `{"version": "0.1.19"}` in its config. A session belongs to the version that opened it, so
  a mismatch reports `not open` against a browser that is alive and still logged in. It
  must be an **exact version** — `1.2.3`, or `1.2.3-rc.1`. Not a range and not `latest`:
  that string is interpolated into an npm package spec, where npm would also accept a
  dist-tag, an alias or a git URL, and a spec that resolves to something new tomorrow is
  the `not open` symptom this override exists to prevent.
- **Whole storage state is not readable this way.** A dump is a credential blob nobody
  declared, and the redactor cannot scrub what it cannot name — name the one key you want.

Every consuming path works unchanged — the value is resolved only when something
actually needs it:

```bash
charter secret exec team --env T=DEPLOY_TOKEN -- deploy.sh
charter secret exec team --dotenv F=TOKEN:DEPLOY_TOKEN -- some-tool
```

| Reference | Resolved with |
| --- | --- |
| `op://<vault>/<item>/<field>` | `op read --no-newline <uri>` |
| `vault://<path>#<FIELD>` | `vault kv get -field=<FIELD> <path>` |
| `browser://<session>/localstorage/<key>` | `playwright-cli -s=<session> --raw localstorage-get <key>` |
| `browser://<session>/cookie/<name>` | `playwright-cli -s=<session> --raw cookie-get <name>` |

Deliberate properties:

- **A bare value is refused.** Storing one would quietly turn a reference vault into a
  plaintext vault — the exact divergence this provider exists to prevent.
- **References are validated when you write them**, so a malformed URI fails as you
  type it, not at 3am when something tries to read it.
- **Resolvers are invoked as argv, never a shell string**, so a reference can never be
  command injection whatever it contains.
- **Redaction covers what comes back, not what the child does with it.** `secret exec`
  scrubs the value from captured output, so a `curl -v` that echoes an `Authorization`
  header is masked. It is `str.replace` over the value's own bytes, so a child that
  *transforms* it — `printf %s "$T" | base64`, `rev`, `gzip`, a POST that never prints it —
  comes back unscrubbed, and no scrubber can win that race: the next encoding is always
  available. `--exec` and `--stream` capture nothing by design, and therefore redact
  nothing — that trade-off is the same for every scheme.
- **`health()` never resolves.** `vault list` and `doctor` call it routinely; resolving
  there would hit 1Password on every listing and could prompt for re-auth.
- **A failed resolve reports status, not output** — a resolver's stderr can echo what it
  fetched.
- **A resolve is bounded** (60s) and a timeout is reported as a named cause rather than a
  traceback. Unattended runs are the reason: a CLI sitting on an authentication prompt with
  nowhere to render it does not fail, it stops — silently, for as long as the session lasts.
- You still need the CLI installed and authenticated; charter shells out to it and says
  so plainly when it is missing.

The reference file is still written 0600. It holds no secrets, but it names your vault
layout, which is not worth publishing either.

The interface (`charter.secrets.base.VaultProvider`) is deliberately small (`get`,
`set`, `delete`, `keys`, `health`) so a keychain- or vault-backed provider can be added
later without touching any call site above it — `charter vault add --provider <x>`
already accepts an unimplemented provider and reports it as "registered for later use"
rather than crashing.

## Feeding a tool that wants a dotenv file

Some tools take a *file* of secrets rather than environment variables. `--dotenv` writes
one 0600 temp file containing every entry you name, points an env var at its path, and
deletes it when the command exits — so no value is ever printed, stored, or placed in
argv.

```bash
charter secret exec qa \
  --dotenv PLAYWRIGHT_MCP_SECRETS_FILE=APP_USER:platform-user \
  --dotenv PLAYWRIGHT_MCP_SECRETS_FILE=APP_PASS:platform-pass \
  -- npx @playwright/cli@0.1.18 -s=login fill e3 APP_PASS
```

Repeats sharing an env-var name merge into a single file, in flag order. Different names
produce separate files. Defining the same NAME twice under one ENVVAR is an error (exit
code 2).

The value is never typed by the caller: the tool refers to the secret by the **name** you
gave it (`APP_PASS`) and resolves it from the file. Any value that does appear in captured
output is redacted.

`--dotenv` cannot be combined with `--exec` — exec replaces this process, so the temp file
would never be cleaned up. Use `--env` for an exec'd command.
