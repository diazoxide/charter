---
version: unreleased
headline: An option is its letter, an assignment is whatever will do the assigning, and an `export` reaches the next segment — three vault-reachable bypasses closed
security: true
---

Three commands printed a fabricated vault, or moved the plane root's HEAD, on a plane built
by `charter init` — each while the spelling next to it was refused. All three are the same
defect wearing three costumes: **a guard matching a spelling instead of the property behind
it.** That is the seventh time this month, so each fix below names its property and is
written against that, and each swept its own siblings before stopping.

Measured against the real `charter hook pretooluse`, one command at a time (the decision is
nested under `hookSpecificOutput`; reading `permissionDecision` from the root answers
nothing for every input and looks exactly like a guard that has stopped working):

| command | before | after |
| --- | --- | --- |
| `cat .charter/vaults/x.json` | DENY | DENY |
| `env -C .charter/vaults cat x.json` | DENY | DENY |
| `env a-b=1 cat .charter/vaults/x.json` | **ALLOW** | DENY |
| `env a.b=1 cat .charter/vaults/x.json` | **ALLOW** | DENY |
| `env 1FOO=1 cat .charter/vaults/x.json` | **ALLOW** | DENY |
| `env -- a-b=1 cat .charter/vaults/x.json` | **ALLOW** | DENY |
| `env -Sfoo=1 -Sbar=2 cat .charter/vaults/x.json` | **ALLOW** | DENY |
| `env -iC.charter/vaults cat x.json` | **ALLOW** | DENY |
| `env -iC .charter/vaults cat x.json` | **ALLOW** | DENY |
| `env -viC.charter/vaults cat x.json` | **ALLOW** | DENY |
| `env -iS'cat .charter/vaults/x.json'` | **ALLOW** | DENY |
| `sudo -bD.charter/vaults cat x.json` | **ALLOW** | DENY |
| `env -P /bin cat .charter/vaults/x.json` | **ALLOW** | DENY |
| `sudo -T 5 cat .charter/vaults/x.json` | **ALLOW** | DENY |
| `chrt 5 cat .charter/vaults/x.json` | **ALLOW** | DENY |
| `su-exec root cat .charter/vaults/x.json` | **ALLOW** | DENY |
| `export GIT_DIR=<plane>/.git && git checkout feature` | **ALLOW** | DENY |
| `export GIT_SSH_COMMAND=/tmp/k && git push` | **ALLOW** | DENY |

Every row marked ALLOW really did the thing: each `cat` printed
`{"k":"FABRICATED-NOT-A-REAL-SECRET-9271"}`, the `export GIT_DIR` line moved the plane
root's HEAD (`git -C <plane> symbolic-ref --short HEAD` answers `feature` afterwards), and
the hook printed nothing in any of them. The two exceptions are the `chrt` and `su-exec`
rows: those are Linux tools this machine does not have, so they come from their documented
grammars rather than from a run here.

## An option is its LETTER, not its position in the token

[#556](https://github.com/diazoxide/charter/issues/556). The parser matched a wrapper's
value-taking flags with `tok.startswith(flag)`, so it only ever saw a short option written
**first**. getopt bundles: `-iC<dir>` is `-i -C <dir>`, and `env`'s `-i` takes no value, so
the `C` after it is the chdir flag and the rest of the token is where the program will run.
`env -iC.charter/vaults cat x.json` relocated into the vault directory, printed it, and was
allowed — while `env -C <dir>`, `env -C<dir>` and `env --chdir=<dir>` were all denied. Three
spellings of one flag refused is what a spelling-shaped guard looks like from the inside.

The guard now walks the letters. Two per-wrapper tables decide each one — the existing
`_WRAPPER_VALUE_FLAGS` (letters that take a value, consulted **first**) and a new
`_WRAPPER_NOVALUE_LETTERS` (letters that take none) — and a letter in neither ends the walk
rather than being guessed at. Guessing "takes a value" swallows the program, which is the
fail-open `env -i cat <vault>` punishes; guessing "takes none" walks into somebody's data.

**And the end of a walk is no longer silent**, which is the part that stops this being a
longer list. An option the grammar could not place leaves the program unnamed — a value flag
nobody has listed is indistinguishable from one that takes nothing — so the rest of the
segment is reported as files the command may open, and the leak guard asks them the same
question it asks a reader's operands. The sweep found two live fail-opens of exactly that
kind and they are the last two vault rows in the table: BSD's `env -P <utilpath>` and
`sudo -T <n>`, both of which had their value read as the program. They are in the value
table now *and* covered by the fallback, so the next missing letter is a false negative
instead of a way in.

**One branch over, the same sweep found a third thing.** `timeout 5 cat <vault>` is the only
wrapper whose leading *positional* was modelled, and the branch that models it reads as if
`timeout` were the only wrapper with one. `chrt [options] <priority> <command>` and
`su-exec <user-spec> <command>` each put an argument of their own in front of the program,
and the parser named that argument as the program — so `chrt 5 cat <vault>` and
`su-exec root cat <vault>` were both allowed. Those two are Linux tools rather than macOS
ones, so unlike every other row above they come from their documented grammars rather than
from a run on the reporting machine; the guard runs on planes where they exist.

## An assignment is whatever will do the assigning

[#555](https://github.com/diazoxide/charter/issues/555). One constant,
`^[A-Za-z_][A-Za-z0-9_]*=`, was answering two different parsers' questions. At the front of
a shell command it is exactly right — bash answers *"a-b=1: command not found"*, so
`a-b=1 cat <vault>` runs a program with that name and reads nothing. `env` is a different
parser reading the same bytes: its operand scan is `strchr(arg, '=')`, GNU and BSD alike, so
**any** argument containing an `=` is an assignment and the scan keeps going until it meets
one without. The guard stopped at the first token it could not read as a shell identifier
and called that token the program.

Widening the constant would have moved denials in both directions, because the same
predicate stands in front of ordinary segments where a token with an `=` is not an
assignment (`git -c a.b=c …`). So there are two predicates now: the shell's rule stays where
the shell decides, and a second one is applied only inside the operand scan of a wrapper that
really does the assigning — `env`, and `sudo`, whose usage prints `[VAR=value]`. Not `doas`,
whose usage has no assignment operand in it: a table that is fail-closed is still a table,
and padding it because padding is safe is the same reflex as a longer list of spellings.
Consuming a token there can only move the
program rightward, never swallow one, so the fail-closed direction is also the correct one.

## An `export` reaches the next segment

[#496](https://github.com/diazoxide/charter/issues/496). The plane-root guards read
`GIT_DIR` / `GIT_WORK_TREE` off the env-assignment prefix attached to a git invocation. An
`export` in an earlier segment of the same command line sets the same variable for the same
shell and reaches the same git, and nothing read it.

`_plane_root_git` already carried one shell effect across segments — a `cd`. The environment
a command line establishes is the same shape of carried state, and it is computed once and
read by **both** plane-root guards and the one-credential guard, because a second
hand-written copy grows its own blind spots on its own schedule. The shapes modelled are the
ones a shell really exports with, each checked against bash 5 and zsh: `export NAME=VALUE`;
`declare -x` / `typeset -x`, whose `x` bundles; `NAME=VALUE` in one segment and `export NAME`
in a later one; and `set -a`, after which a bare assignment segment is exported too. A bare
`NAME=VALUE` segment on its own is deliberately **not** — `FOO=1; <child>` leaves `FOO`
unset in the child, so treating it as an export would be a denial invented out of nothing.

That environment only ever grows. `unset`, `export -n` and a subshell ending are not
modelled, because forgetting a variable is the direction that opens a door — the same
invariant `_git_target`'s subject list keeps. The boundary is `cd`'s boundary: a `$(…)`, a
sourced file, and a `GIT_DIR` already in the session's environment before the hook ran are
outside it, and for the last one the `PreToolUse` payload carries the command and the cwd,
not the environment the command will inherit. Those are stated limits with tests on them,
not gaps.

**The sibling this one found.** The golden rule's one-credential guard reads the same env
prefix, and had the identical hole:
`export GIT_SSH_COMMAND=/tmp/k && git push` walked past it while
`GIT_SSH_COMMAND=/tmp/k git push` was refused. Nobody had reported it. Both guards are wired
to the same carried environment, so neither can be fixed while the other is left behind —
which is how `GIT_DIR` came to be refused in one spelling and allowed in the next.

## Three lines deleted, and one table row that had no grammar behind it

`tools/sweep.py` and the hand-check beside it — the sweep has no operator for a string table
or a parse rule, so those are mutated by hand — killed three guards and a table row, and
nothing noticed. A differential then showed why for each, and each is gone:

* two guards inside the letter walk. Over 6.9 million `(wrapper, token)` pairs, neither one
  alone nor both together changes a single answer: a long option's first walked character is
  the second `-`, which is in no wrapper's no-value letters, so the walk already ends on its
  first step; and the walk asks `"-" + ch in takes`, which is two characters, so filtering
  the table down to two-character spellings first could only ever have removed `--` itself.
* a "skip the flags" branch in the export scan. Every shell-variable key comes from the
  identifier regex, so a `-…` token can neither be recorded as a variable nor found as one —
  213,927 segment lists, no divergence.
* `doas` in the list of wrappers whose operands can be `VAR=value`. Its usage is
  `doas [-Lns] [-a style] [-C config] [-u user] command [args]`, with no assignment operand
  in it at all. That one was not equivalent — it was wrong, and it was invented by symmetry
  with `sudo`. A fail-closed table is still a table.

Every one of those deletions rests on a fact, and every one of those facts is now an
assertion, because "it happens to be equivalent today" is how a deleted guard comes back to
life as a bypass.

**The opposite finding is the more useful one**, and there were five: the `()` default on the
value-flag lookup (without it `nohup -q cat <vault>` raises out of the leak guard, and a
guard that raises is a guard that is not there); the `and toks` that stops `env -C` at the
end of a segment popping from an empty list; the `except ValueError` that keeps an
unbalanced `env -S "cat '<vault>"` from doing the same; the `base == "env"` beside the
split-string test, without which `xargs -S 4096 cat <vault>` — `xargs` has a `-S` of its own,
and it means something else — would unpack `4096` as a command and lose the reader; and the
test that keeps a *separated* value from reporting its whole segment, which is what lets
`env -C /tmp echo <vault>` stay allowed, because printing a path is not reading a file.

## What did not change

`SECURITY.md`'s position is unmoved: guard rails, not guarantees. Deciding what a shell will
execute, without executing it, is not winnable in a Python tokeniser, and none of this is an
attempt to close that class. What it does close is three cases where the guard already
modelled the thing and was matching one way of writing it — plus the sweep's four, which
nobody had reported.

Nothing to adopt: upgrading is the whole of it.

[#555](https://github.com/diazoxide/charter/issues/555),
[#556](https://github.com/diazoxide/charter/issues/556),
[#496](https://github.com/diazoxide/charter/issues/496), each confirmed live on `main` by
reproduction against a plane built by `charter init` before anything was changed.
