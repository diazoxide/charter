---
name: secrets
description: Use a credential held in a charter vault — a database password, API token, kubeconfig, SSH key or server login — without its value entering the conversation. Use when a task needs a secret, when asked to store one, or before running any command that requires a credential.
---

# Using a charter vault

The rule this exists to keep: **use a secret, never reveal it.** A value that reaches the
transcript is disclosed — to the model's context, to whatever logs the session, and to
anyone the transcript is later shared with. Deleting the message afterwards does not undo
any of that.

Full model, including what the vault does *not* protect against: `charter docs show secrets`.

## Find out what exists

```bash
charter vault list                 # vaults: name, provider, persona, status — no values
charter secret list <vault>        # the KEYS in one vault — no values
```

## Store one — the value never goes on the command line

```bash
printf '%s' "<value>" | charter secret set <vault> <key> --stdin
charter secret set <vault> <key> --from-file <path>    # multi-line or verbatim: kubeconfig, PEM
```

An argument list is not private: it is visible in `ps`, in shell history, and in this
transcript. Ask the user to supply the value by stdin or file, or to set it themselves.

## Use one — pick an injection path

**As an environment variable:**

```bash
charter secret exec <vault> --env NAME=<key> -- <command...>
```

**As a file** (kubeconfig, certificate, key):

```bash
charter secret exec <vault> --file KUBECONFIG=<key> -- kubectl get pods
charter secret cp <vault> <key> <dest>     # persist at 0600; prints only the path
```

`<dest>` must be a **real file that does not exist yet**. A device, a FIFO, a directory
or a symlink is refused, and so is an existing file (overwriting takes `--force`). This
is not pedantry: `charter secret cp <vault> <key> /dev/stdout` would write the plaintext
straight into this conversation — that is the one thing the vault exists to prevent, so
do not go looking for a path that gets around the refusal.

There is not one to find. A destination that is charter's own stdin, stdout or stderr is
refused by identity — the inode, not the spelling — so `/dev/fd/1`, `/proc/self/fd/1`,
the transcript's real path and any hardlink to it are one object with one answer, and
`--force` does not reach that check.

**As a dotenv file**, for a tool that reads one (this is how a browser driver gets a
login without the password being typed into the page by you):

```bash
charter secret exec <vault> --dotenv ENVFILE=USER:<key>,PASS:<key> -- <command...>
```

In every case the value is injected into the subprocess and **redacted from its output**,
so a command that echoes it still cannot leak it into the transcript.

**Just checking one is present:**

```bash
charter secret get <vault> <key>       # masked: size band + keyed fingerprint
```

The fingerprint is keyed to this control plane, not a hash of the value, so it cannot be
checked against a guess — and the size is a band, not a count. Compare two of them to ask
"same value?"; there is nothing else to do with one, and nothing to be gained by
recomputing it.

## Hard rules

- **Never `charter secret get --reveal`.** It refuses a non-interactive stdout by design.
  Forcing it puts the value in context, which is the one outcome the vault exists to
  prevent. Use `exec` or `cp`.
- Never echo a secret, write it into a tracked file, or pass it as a literal argument.
- Never put a secret in memory, a persona charter, a workspace charter, or a commit
  message. The vault is the only place for one.
- If the vault or key does not exist, say so and ask for it to be added — do not work
  around it with a value pasted into the conversation.

## Working as a persona

A persona owns a vault. When one is active, prefer the persona form — same commands,
resolved against the active persona's vault, no vault name to get wrong:

```bash
charter persona secret list
charter persona secret exec --env TOKEN=<key> -- <command...>
```

A persona can only reach its own vault. When a task needs a credential another persona
holds, delegate that step to that persona rather than copying the secret across — it runs
with its own vault and you never see the value.
