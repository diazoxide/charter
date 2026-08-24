---
version: unreleased
headline: `.charter/` is 0700 because charter says so, not because of your umask
---

`.charter/` holds the vault registry, `fingerprint.key`, and every plain-file vault this
plane keeps. Until now the mode of that directory was decided by whoever created it, at
`0o777 & ~umask` — 0755 on the default `umask 022`. The vault writers walked every level of
a vault path at 0700, but on the flow `charter vault add` prints as its own first step they
are not what creates the state directory: the local registry write is, through a bare
`mkdir(parents=True, exist_ok=True)`. So did the first SessionStart hook in a freshly cloned
plane (`.charter/` is gitignored, so a teammate's clone has none), and so did the PreToolUse
hook, and so did the status line's cache. Whichever you happened to run first decided
whether every account on the machine could list the plane's state directory
([#470](https://github.com/diazoxide/charter/issues/470)).

The walk now lives in `config.private_mkdir` — `secrets.base.make_private_dir` is the same
function under the name the vault writers already used — and **every writer that creates a
directory under `.charter/` goes through it**: the registry, the persona and workspace
pointers, the session and terminal markers, the caches, the dispatch tracker, the frame,
the trace and the reports. Each missing level is created individually and chmod-ed to 0700,
because `mkdir(parents=True, mode=0o700)` applies the mode to the leaf only and `mkdir`'s
mode argument is masked by the umask anyway.

`umask 000`, `umask 022`, `umask 077`: `.charter` comes out 0700 in all three, on `vault
add`, on the SessionStart hook and on the PreToolUse hook. The property being tested is
that the umask does not decide it — a fix that only held under the umask it was written for
would satisfy "the mode is 0700" and nothing else.

**A directory that was already there keeps its mode, and is still reported.**
A `.charter/` that predates this, or one made by `mkdir -p` at the umask default, is 0755
before and 0755 after. Charter tightens what it creates and names what it did not: a
vault's `file` may point anywhere on this machine and `$CHARTER_HOME` may move the state
directory anywhere, so "chmod whatever we land in" is how charter would come to tighten a
home directory or a shared team directory unprompted.

**That report now reaches `charter doctor`**
([#471](https://github.com/diazoxide/charter/issues/471)). It used to exist only inside the string
`health()` returns — which `charter vault list` prints as its STATUS column and `doctor`
threw away, keeping just the reachable/not-reachable boolean. `doctor` now asks the
provider for the directories themselves and renders them with the same function `vault
list` uses, so the two cannot drift apart:

```
  ✓  vaults           1 reachable (references not resolved); listed by other accounts: .charter 755 (want 700 — chmod 700)
```

Green, not a warning, and on the JSON too. Charter will not fix this one for you, so saying
it *is* the remedy — and a warning at every session start about a directory you have decided
to leave alone is a check people learn to skip, which takes the rest of the report with it.
It also rides the warning paths: a vault that times out or has no identity is a different
problem, and the loose directory does not stop being one while that is being fixed.

**Known remaining case: a `reference` vault reports no directory.** It writes its file
under `.charter/vaults/` through the same private walk, so nothing it creates is loose —
but if that directory predates charter, neither `vault list` nor `doctor` says so for a
plane whose only vaults are references. The base provider's honest answer is an empty list
rather than a guess about a backend charter does not own; teaching the reference provider to
answer properly is [#491](https://github.com/diazoxide/charter/issues/491).

Two things this does not do. It does not read ACLs: on macOS `chmod +a` and on Linux
`setfacl` grant another account access while `st_mode` still reads 0700, and a POSIX mode
is all charter looks at. And it does not encrypt anything — a plain-file vault stores values
in the clear, which is documented in four places and remains the trade-off for the provider
whose entire job is "a JSON file you can also edit by hand".
