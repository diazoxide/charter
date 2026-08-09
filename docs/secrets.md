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

- **`charter secret cp`** materializes a secret to a 0600 file (e.g. a kubeconfig) and
  prints only the path, never the contents.
- **`charter secret get`** is masked by default — it prints a byte count and a SHA-256
  fingerprint, never the value.
- **`charter secret get --reveal`** is the one path that *can* print plaintext, and it
  deliberately refuses to do so to a **non-interactive stdout** (the exact channel
  through which a value would leak straight into an agent's context) unless you pass
  `--force` — it's meant for a human at a real terminal, not a script or an agent.
- Values are always **written** via `--stdin` or `--from-file`, never as a bare CLI
  argument — an argument shows up in shell history and `ps` output for any other
  process on the machine to read.
- A Claude Code guard hook denies `--reveal` outright, and denies reading a vault file
  directly (`cat .charter/vaults/…`) — both would print a secret straight into the
  conversation. **A denial here is that guard working, not a bug** — see the README's
  "one credential" section for the same idea applied to git auth.

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
| 1Password (native) | `1password` | Implemented. charter **creates and manages** the items via the `op` CLI. |

If 1Password is where your credentials belong, you have two shapes to choose between,
and the difference is who owns the item:

- **`1password`** — charter owns it. `secret set` creates the item, `rm` deletes it, so
  a credential can be provisioned for a persona without opening the 1Password UI.
- **`reference`** — someone else owns it. charter stores only a pointer to an item that
  already exists. Right when the credential is shared with people or systems beyond
  charter, or when a human should stay in charge of rotating it.

### Native 1Password vaults

```bash
charter vault add devops --provider 1password --op-vault Engineering --persona devops
charter secret set devops KUBECONFIG --from-file ~/.kube/prod.yaml
charter secret exec devops --file KUBECONFIG=KUBECONFIG -- kubectl get pods
```

Schema — **one 1Password item per secret**, not one per vault:

| charter | 1Password |
| --- | --- |
| vault `devops`, key `KUBECONFIG` | item `charter-devops-KUBECONFIG` |
| | tagged `charter`, `charter:devops` |
| | value in the item's `password` field |

One item per *vault* is the tidier-looking design and it is wrong here. Updating one
field of a multi-field item means a read-modify-write through a JSON template, a
template **replaces** the item rather than merging, and `op item get --format json`
*conceals* values unless asked otherwise — so round-tripping would overwrite every
sibling secret with a mask. One item per key removes the interaction: each write
touches exactly the credential it was asked to touch.

Deliberate properties:

- **No secret ever reaches argv.** 1Password's own help warns that "command arguments
  get logged in your command history, and can be visible to other processes on your
  machine". Writes pipe a JSON template on **stdin** (`op item create -`,
  `op item edit <item>`); only names are ever passed as arguments.
- **charter lists only what charter created.** `secret list` filters by the
  `charter:<vault>` tag, so a shared 1Password vault your team also fills by hand is
  never listed — far less offered for deletion.
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

Deliberate properties:

- **A bare value is refused.** Storing one would quietly turn a reference vault into a
  plaintext vault — the exact divergence this provider exists to prevent.
- **References are validated when you write them**, so a malformed URI fails as you
  type it, not at 3am when something tries to read it.
- **Resolvers are invoked as argv, never a shell string**, so a reference can never be
  command injection whatever it contains.
- **`health()` never resolves.** `vault list` and `doctor` call it routinely; resolving
  there would hit 1Password on every listing and could prompt for re-auth.
- **A failed resolve reports status, not output** — a resolver's stderr can echo what it
  fetched.
- You still need the CLI installed and authenticated; charter shells out to it and says
  so plainly when it is missing.

The reference file is still written 0600. It holds no secrets, but it names your vault
layout, which is not worth publishing either.

The interface (`charter.secrets.base.VaultProvider`) is deliberately small (`get`,
`set`, `delete`, `keys`, `health`) so a keychain- or vault-backed provider can be added
later without touching any call site above it — `charter vault add --provider <x>`
already accepts an unimplemented provider and reports it as "registered for later use"
rather than crashing.
