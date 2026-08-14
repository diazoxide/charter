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

The same guard covers the vault: it refuses `--reveal` on a non-interactive stdout and
refuses file-reading tools pointed at a vault file, so a secret cannot reach the transcript
by way of `cat`. See [secrets.md](secrets.md).

## When you are not using charter for git

The policy is applied per repo, in that repo's local config, and never to your global git
config. A repo charter has never cloned or applied policy to is untouched — your own
day-to-day SSH setup elsewhere on the machine keeps working exactly as before.
