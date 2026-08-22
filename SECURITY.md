# Security policy

## Reporting a vulnerability

Please **do not open a public issue** for a security problem.

Report it privately **by email to the maintainer address in
[`pyproject.toml`](pyproject.toml)** (the `authors` line). Include the charter version
(`charter version`), your OS, and the smallest reproduction you have.

GitHub's private vulnerability reporting is not enabled on this repository, so its
advisory form is not a route here — email is. This page names only the channel that
works, because a report filed into a channel nobody is listening on is indistinguishable,
from the maintainer's side, from no report at all.

You should get an acknowledgement within a few days. charter is maintained by one person,
so please allow reasonable time for a fix before disclosing publicly.

## Supported versions

charter is pre-1.0 and moves quickly. Fixes land on the latest release; there are no
maintained backport branches. Upgrade with `uv tool upgrade charter-cp` (and
`claude plugin update charter@charter`, which is a separate artifact with its own version).

## What charter's vault does and does not protect

This is the part most worth reading before you trust charter with anything real, and the
[full write-up is in docs/secrets.md](docs/secrets.md).

**What a vault protects against — and this is the whole point:** a secret value reaching
the model's context window, and from there the transcript, the logs, and any summary fed
into a later prompt. `charter secret exec` resolves the value inside charter's own process,
places it in a child command's environment, and redacts every occurrence of it from
whatever that command prints. The model names the secret; it never sees it.

**What a vault does not protect against.** The default provider stores values as
**plaintext JSON at file mode 0600**. There is **no encryption at rest**. Anyone who can
read that file as your user — or restore it from a backup, or read the disk — has the
secret. It is not a password manager, and it is not a substitute for one. For secrets that
warrant real custody, use the `1password` or `reference` providers, which keep the value in
a system built for it and resolve it on demand.

**Guard rails, not guarantees.** The Claude Code plugin's `PreToolUse` guard denies
`--reveal` on a non-interactive stdout and denies file-reading tools pointed at a vault
file. That closes the easy accidental paths. It is a guard against mistakes, not an attacker
with shell access as your user.

## The one-credential rule

Every git operation charter performs authenticates with the forge CLI's token over HTTPS —
never an SSH key, never signing. The plugin's guard denies commands that would route around
it. Details, and why a denial there is the rule working rather than a bug, are in
[docs/git-policy.md](docs/git-policy.md).

## Scope

In scope: anything that leaks a vault value into a model context, transcript or log;
anything that lets a repo's contents escalate into command execution on the host; anything
that bypasses the vault guard or the git policy guard.

Out of scope: the plaintext-at-rest property of the default vault provider, which is
documented above and deliberate; and the behaviour of `gh`, `glab`, `op` or `vault`
themselves.
