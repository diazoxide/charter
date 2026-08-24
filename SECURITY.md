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
into a later prompt. `charter secret exec` resolves the value inside charter's own process
and places it in a child command's environment. **The model names the secret and never
types it.**

**What that depends on, and it is not a footnote.** The model chooses the command charter
runs. Redaction scrubs the value out of *captured* output, so a `curl -v` that echoes an
`Authorization` header is masked — that is a net against an accidental echo, not a
boundary. It is `str.replace` over the bytes that came back
([`charter/secrets/base.py`](charter/secrets/base.py)), so a command that *transforms* the
value is not scrubbed and cannot be: `secret exec v --env T=K -- sh -c 'printf %s "$T" |
base64'` returns the credential in full, and so does `rev`, `fold -w1`, or a `curl -d` that
never prints it at all. `--exec` and `--stream` capture nothing and therefore redact
nothing. So the guarantee is precisely this, and it is narrower than one sentence: **on
the paths that consume a value — `secret exec` with `--env`/`--file`/`--dotenv`, and the
MCP launcher — charter never prints the value into the conversation, and everywhere else
charter prints it only into a destination you named yourself.** Where the value goes after
that is a property of the command you asked charter to run. Read `charter secret exec
<vault> -- <cmd>` with the same suspicion you would read `<cmd>` holding the credential
directly, because that is what it is.

**Which destinations charter will take, and the one that still prints.** Two commands put
a plaintext value somewhere you can read it. `charter secret cp <vault> <key> <dest>`
writes a **real file it creates**; it used to accept `/dev/stdout` and print the
credential into this transcript in charter's own process, and that is fixed
([#449](https://github.com/diazoxide/charter/pull/449)) — a symlink, a device, a FIFO and
an existing file are each refused for their own reason, and a destination that turns out
to **be** one of charter's own streams is refused whatever it is called, because the test
is `(st_dev, st_ino)` from an `fstat` of the descriptor charter opened compared against
its own stdin, stdout and stderr. `--force` does not reach that check.

`charter secret get --reveal` is the one path left where charter's own process writes a
value to its own stdout, and it refuses a non-interactive stdout — an agent's pipe —
unless you pass `--force`. That refusal is the whole of the protection: `--force` is a
real override, and an agent that has a shell can type it. Do not hand `--reveal --force`,
or a `cp` destination, to something an agent chose.

**What a vault does not protect against.** The default provider stores values as
**plaintext JSON at file mode 0600**. There is **no encryption at rest**. Anyone who can
read that file as your user — or restore it from a backup, or read the disk — has the
secret. It is not a password manager, and it is not a substitute for one. For secrets that
warrant real custody, use the `1password` or `reference` providers, which keep the value in
a system built for it and resolve it on demand.

**Guard rails, not guarantees.** The Claude Code plugin's `PreToolUse` guard denies
`--reveal` on a charter invocation it can recognise, and denies a known file-reading
program — `cat`, `grep`, `head` and a dozen more — whose argument, *as the model wrote it*,
spells one of the **guarded paths**: the state directory `.charter` itself, the vault
directory `.charter/vaults` and anything inside it, `.charter/browser…` and
`.charter/active-…`. Not every path under `.charter/` — `.charter/state/…` and the config
beside it are ordinary reads, and so is `.charter/vaults.json`, the registry, which holds
provider config and file paths but never a value. That qualifier *as the model wrote it* is
the whole of the guard's ceiling and is not decoration: it is a text match on a command
line, run before any shell touches it. It
does the same for the harness's own `Read` and `Grep` tools, on every harness charter
supports: Claude Code and Codex dispatch both handlers from the plugin's `hooks/hooks.json`,
and opencode from the plugin `charter init` generates. That closes the easy accidental
paths. It is a guard against mistakes, not an attacker with shell access as your
user, and the shape of that limit is worth knowing rather than guessing at:

- It matches **program names it knows**, so `python3 -c "print(open('.charter/vaults/db.json').read())"`,
  `base64 .charter/vaults/db.json`, `cp … /tmp/x`, and `git show HEAD:.charter/vaults/db.json`
  all run. Widening the list does not fix this — the next name is always the missing one.
- It reads the **argv it is given**, and does not re-parse a shell string, so
  `sh -c 'cat .charter/vaults/db.json'` is one opaque argument to it.
- It matches the **text of the operand as written**, normalised first. Redundant `/`
  separators, `.`/`..` segments and letter case are all collapsed, so
  `.charter//vaults/db.json`, `.charter/./vaults/db.json` and `.CHARTER/vaults/db.json` get
  the same answer as `.charter/vaults/db.json` — the last of those matters because macOS
  folds case in the filesystem by default, and it is the same file there. The directory
  itself counts, with or without a trailing slash, because `grep -r TOKEN .charter/vaults`
  walks every vault file in it. Two things it does **not** know, and they are limits rather
  than bugs:
  - A *different* path holding the same bytes: a vault registered outside `.charter/` (which
    is what `charter vault add --file` offers when the default location would be committed),
    a file `charter secret cp` wrote somewhere you named, or a symlink you planted. Resolving
    those would mean a `stat` on every operand of every command.
  - A separator that is not `/`. Normalisation is POSIX; a Windows-style
    `.charter\vaults\db.json` is a different path on POSIX, where a backslash is an
    ordinary filename character, so folding it here would deny real filenames. **charter's
    harness targets POSIX** — macOS and Linux, with tmux — and is neither tested nor
    supported on Windows, which is why the guard is not made to answer differently there.
    If that changes, this fold has to change with it (#476).
- A **second predicate** covers the operand that *contains* the vault directory without
  naming it (#474). `grep -r TOKEN .` from the plane root reads the vault directory as
  collateral while its operand is the single character `.`, which leaves a guard that reads
  only operand text with nothing to match on. Both routes now also ask whether the walk
  would **reach** charter's own state:
  the operand is resolved against the shell's directory and compared by ancestry, and the
  entries it is compared against are read off the filesystem, so `.`, `..`, an absolute
  path and a symlinked parent are one question rather than four spellings. It fires only
  when those entries exist and hold something, and the denial names the exclusion that
  fixes it (`--exclude-dir=.charter`, `rg --glob '!.charter'`). What it still does not
  cover is the same ceiling as above: **a program charter does not know walks directories**
  — `find … -exec cat`, `tar`, an interpreter — reaches the same files unguarded. And
  `charter init` gitignores the whole of `/.charter/`, which is what actually keeps a vault
  out of a commit.
- **Anything a shell does to that text, it does after the guard has answered.** The hook is
  handed the command line before `sh` runs and never sees what `sh` turns it into, so every
  expansion reaches the file unguarded: a glob (`cat .charter/vault?/db.json`,
  `head .charter/vault*/db.json`, `cat .cha*ter/vaults/db.json`), a variable
  (`V=.charter/vaults/db.json; cat $V`), a quoted command substitution, brace or tilde
  expansion. Each is `cat` on the same inode as the denied form, one keystroke away from it,
  and allowed. A changed working directory (`cd .charter/vaults && cat db.json`) used to be
  named here as one of them and is **not** one: the guard walks the segments and carries the
  relocation, however it is spelled (`cd`, `pushd`, `env -C`, `sudo --chdir`), so those are
  denied — this page said otherwise while `docs/secrets.md` said the opposite on the same
  day. This is not a
  list of tricks to close one by one — it is one fact with many spellings, and closing them
  means putting a shell inside the guard, which leaves a hole shaped like whichever
  construct that shell got wrong. The glob case has an exact edge worth knowing: the
  metacharacter has to fall inside `.charter/vaults/` itself, so
  `cat .charter/vaults/*.json` is still denied.

None of that is a reason to switch the guard off, and none of it is a bug report: it is why
the sentence above says *mistakes*. The boundary that does not depend on a name is that
charter itself never prints the value.

**On opencode the ceiling is lower than that, and it is worth naming.** opencode loads
every file in its `plugin/` directory into one JavaScript realm with shared globals, so a
second plugin installed there can redefine what charter's plugin calls and silently disable
its guards — verified against opencode 1.18.21 with a six-line neighbour that turned a
`read` of a vault file into an allow. charter cannot prevent that from Python. What it does
is report it: anything in that directory charter did not write is named on the `charter
doctor` harness row and in `charter harness list`. Reporting the realm, not containing it.


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
