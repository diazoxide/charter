---
version: unreleased
headline: A repository that names its own work tree cannot use it to reach the plane root — `core.worktree` is the third spelling, and the plane-root guards read it now
security: true
---

`--work-tree` and `GIT_WORK_TREE` are tokens. `core.worktree` is not: it is a key in a
repository's own `.git/config`, and a repository carrying it has the named directory as its
working tree for **every** command it runs. So the plane-root guards, which read a
command's argv and environment, saw a plain `git checkout feature` typed inside a workspace
clone — and git wrote that branch's content into the plane root.

Reproduced end to end on git 2.50.1, with the plane's file changing:

```
git clone <plane> /tmp/cfgclone
git -C /tmp/cfgclone config core.worktree <plane>
git -C /tmp/cfgclone rev-parse --show-toplevel      # -> <plane>
git -C /tmp/cfgclone checkout feature               # -> <plane>/f.txt is the branch's now
```

`_plane_root_branch_reason` and `_plane_root_reset_reason` both answered `None`, because
every subject `_git_target` reported was inside the clone.

`_git_target` gained the fourth subject, and its invariant is unchanged: the list only ever
grows, and the cwd is always in it. Three routes are in
`tests/test_plane_root_checkout_is_two_commands.py`'s corpus now, crossed with every command
it already carries — the key read from the cwd's repository, and the same key reached from
outside that repository by `--git-dir` and by `GIT_DIR`, both of which take the work tree
from the config of the repository they name. The row that used to pin this as a **limit** is
flipped to a denial in the same file.

**What it costs, because that is why #497 filed this rather than doing it.** Following
`core.worktree` means reading a config on the hot path, where the guard's common case exits
on a string comparison. `git rev-parse --show-toplevel` answers the question exactly and
costs a subprocess — some ten milliseconds inside a hook that runs on every Bash call — so
charter reads the file instead: a walk up to the repository, and one read of a file that is
under a kilobyte in every repository anybody has. Measured: **14 µs** where there is no
repository above the cwd, **41 µs** for a repository with no such key (the ordinary case),
**76 µs** where the key is there. `charter/gitconfig.py` owns the read and the number, and a
test asserts no process is started to answer it — a wall-clock ceiling on a fast machine
would not notice a later simplification onto `git rev-parse`.

**git itself is the oracle for the reader.** Charter parses the config rather than asking
git, so the thing that can go wrong is charter's reader disagreeing with git's. Every value
form is therefore checked against `git rev-parse --show-toplevel` on the same file:
absolute, relative (which resolves against the **git directory**, not the work tree —
`../plane` is the value that looks right and makes git refuse the repository outright),
quoted and unquoted values holding a space, a trailing comment, trailing whitespace, a
capitalised section header, a value continued over a backslash, a repeated key, and
`[core "x"]`, which is a different key and relocates nothing.

**Three routes are deliberately not followed**, each recorded rather than implied away:

* `git -c core.worktree=<dir>` on the command line. Git ignores it — verified in both
  spellings, with and without an explicit `--git-dir` — so the form an agent would actually
  type reaches nothing, and reading it would only manufacture refusals. There is a test that
  asks git this before asserting it.
* `include` / `includeIf` directives, which would mean a second file read per invocation
  with git's own `gitdir:`/`onbranch:` matching behind it.
* The global and system configs, where git honours `core.worktree` only when `$GIT_DIR` is
  set.

Every way the read can fail answers *no work tree named*, which leaves the guard exactly as
strong as it was before this existed. That is the fail-open direction and it is the honest
one: this ADDS a subject, so a missing answer costs coverage while an invented one would
refuse a command with nothing wrong with it.

`SECURITY.md`'s position is unmoved — guard rails, not guarantees. A repository's config has
to already say this, so it is not the ordinary mistake the plane-root guards exist to catch;
it is closed because a workspace clone's config is exactly the kind of thing a repo's own
tooling writes.

Nothing to adopt: upgrading is the whole of it.

[#504](https://github.com/diazoxide/charter/issues/504).
