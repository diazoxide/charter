# The one-credential rule

Every git operation charter performs — from any persona, any sub-agent, any repo clone —
authenticates with **that repo's own forge's CLI token, over HTTPS**: `glab` for GitLab,
`gh` for GitHub. Never an SSH key, never commit or tag signing.

```bash
charter git-policy            # check every clone against the rule
charter git-policy --apply    # write it into each repo's local git config
```

`--apply` writes a credential helper, `commit.gpgsign = false`, and SSH→HTTPS URL rewrites
into a repo's *local* git config, so even a repo whose remote is an SSH URL still
transports over HTTPS. `charter clone` applies it automatically to everything it clones.

## Why it is a rule and not a preference

An **SSH key passphrase prompt or a GPG signer prompt hangs an autonomous agent** mid-run.
There is no human at the keyboard to answer it, so the run does not fail — it sits there
until something times out, which is a worse failure than an error because nothing reports
it. One credential, held by the forge's own CLI, over HTTPS, is the only shape that can
never block on a question nobody is there to answer.

It also collapses credential management to one place. The forge CLI already handles
storage, refresh and revocation; charter borrows that rather than inventing a second
system that would need its own rotation story.

## The guard

The Claude Code plugin's `PreToolUse` guard **denies** a command that would route around
the rule:

- a raw SSH GitLab/GitHub URL handed to git
- `GIT_SSH_COMMAND=`
- `-S` / `--gpg-sign`
- `ssh -T git@github.com`

**If you hit one of these denials, that is the rule working, not a bug.** The message names
the fix, which is usually nothing at all — `charter git-policy --apply` has already
configured the repo correctly. Check the credential with `glab auth status` or `gh auth
status`, never `ssh -T`.

**And when it is not the rule working** — a repo that genuinely needs something this guard
refuses — see [hooks.md](hooks.md) → *When a guard is wrong*. Short version: nothing in
`charter.toml` or the environment lifts a denial, deliberately, and you run the command in
your own terminal. If the guard is wrong about you *every time*, that is charter holding a
policy your organisation does not, and it belongs in an issue rather than a local switch.

The same guard covers the vault: it refuses `--reveal` on a non-interactive stdout and
refuses file-reading tools pointed at a vault file, so an accidental `cat` cannot put a
secret in the transcript. Those are the accidental roads, and they are the only ones a
name-based guard can close — a command you chose to run is not one of them. See
[secrets.md](secrets.md) and [SECURITY.md](../SECURITY.md).

## Submodules are outside the rule, and charter says so rather than reaching past it

`charter clone` clones the repo you named and **does not initialise its submodules**.
`charter clone`, `charter sync` and `charter status` each say when a clone has submodules
with nothing checked out, name them, and print the one command that fixes it:

```bash
git -C workspaces/<ws>/<repo> submodule update --init --recursive
```

It is yours to run, and that is a decision rather than an omission.

**A submodule URL is not a URL charter built.** It comes out of `.gitmodules`, a file
inside the repo that was just cloned, and it can name any host, recursively.
`commands._https_url` already refuses to hand `git clone` a string charter did not
build — `ext::sh -c '…'` is a transport that runs a command — and fetching whatever
`.gitmodules` names would put that string back one layer down, where the allowlist above
it cannot see it.

**And the policy on this page does not reach a submodule fetch anyway.** Everything
`--apply` writes goes into a repo's *local* config, and **`git clone` does not read the
local config of the repository it is standing in** — system, global and `-c` only. A
submodule fetch *is* a nested `git clone`. Measured on git 2.50.1, against a superproject
whose submodule cannot be fetched at all without the config under test:

| where `protocol.file.allow` was set | `git submodule update --init` |
| --- | --- |
| the superproject's **local** config | fails — never read |
| `-c` on the command line | succeeds |
| **global** config | succeeds |
| **local** `submodule.<name>.url` override | succeeds |

The last row is the asymmetry: the *parent* (`git submodule update`) resolves the URL from
local config, so a `submodule.<name>.url` override works; the *child* (`git clone`)
consumes `credential.helper` and `url.<https>.insteadOf`, and its config search skips the
local file that holds them. So a submodule fetch runs **without** charter's credential
helper and **without** its SSH→HTTPS rewrite, whoever starts it. A charter that
initialised submodules for you would be fetching outside its own credential policy,
quietly, with your token in reach.

Two practical consequences if your repos keep tooling in a submodule:

- A submodule whose `.gitmodules` URL is SSH will go over SSH — the rewrite is not there.
  `git config submodule.<name>.url https://…` in that clone is read by the parent and
  fixes it for good.
- The guard is **not** what stops you. It reads the command line, never `.gitmodules`, so
  `git submodule update --init` is not denied by anything. What it *does* deny is typing an
  SSH URL while you configure the override — write the HTTPS form instead.

## When you are not using charter for git

The policy is applied per repo, in that repo's local config, and never to your global git
config. A repo charter has never cloned or applied policy to is untouched — your own
day-to-day SSH setup elsewhere on the machine keeps working exactly as before.
