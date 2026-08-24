---
version: unreleased
headline: The plane-root and vault guards follow what a command does, not how it is spelled
---

Four holes, one shape. Each of the guards below already knew about the thing that walked
past it — `--git-dir` was in the guard's own table of git options, `-C` was in its own list
of branch-creating flags, the alias resolver was two hundred lines up the same file — and
each read that thing in one role and never in the other. That is not a missing feature; it
is a guard matching a **spelling** where the property was available to it.

**`git switch -C <branch>` created a branch in the plane root and nothing said anything**
([#483](https://github.com/diazoxide/charter/issues/483)). `-C` is git's own
change-directory global *and* `switch`'s `--force-create`, and the guard stripped the first
reading from anywhere in the argv — so `git switch -C neu` was read as a command aimed at a
directory called `neu`, which is not the plane root, and both plane-root guards stood aside
without printing a word. git answered *"Switched to a new branch 'neu'"*. The attached
spelling `-C<branch>` was refused the whole time, which is what a spelling-shaped guard
looks like from the inside: one keystroke decided whether the guard could see the command.

The fix is a split, not a list. A `-C` counts as git's global only **before the
subcommand** — the only position git itself reads one in — so nothing here has to know
which options belong to `switch`, and the next subcommand option that collides with a git
global is already covered.

**`git --git-dir=<plane>/.git checkout <branch>` moved the plane root's HEAD from a
workspace clone** ([#477](https://github.com/diazoxide/charter/issues/477)). `--git-dir`,
`--work-tree` and their `GIT_DIR` / `GIT_WORK_TREE` environment spellings all name a
repository without naming a directory to stand in. All of them were already in the guard's
table of git options — where they were skipped as option *values* so they could not be
misread as a subcommand, and then never looked at again. The guard now asks which
repository an invocation names rather than "the cwd unless `-C` says otherwise", and answers
with every subject the command has: a `--git-dir` with no `--work-tree` beside it moves the
named repository's refs while its files land in the cwd, so both count. Attached and
separated forms, composed with `-C` (git applies `-C` first, and a relative `--git-dir` is
read against where it landed), and the environment spellings — all of them are now rows in
the guard's corpus, crossed with the commands rather than listed beside them.

**A `git reset --hard` hiding behind an alias destroyed unpushed commits in the plane
root** ([#467](https://github.com/diazoxide/charter/issues/467)). The branch guard has
followed aliases since 0.51 — `co = checkout` is on a large share of developer machines —
and the history-wipe guard, the one that exists because eleven memory commits were lost in
one session, was still comparing the subcommand to the string `reset`. So
`git -c alias.z='reset --hard origin/main' z` went through, and so did `git wipe
origin/main` with `wipe = reset --hard` in the repo's config. Both guards now resolve the
alias before deciding. The cheap early exit went with it: testing for the word `reset` in
the command line is sound only while the subcommand is read as written, and those five
characters live in the config for a config alias. It tests for `git` instead, which is
sound whatever the subcommand is called, because nothing below it can deny without a `git`
invocation.

**`grep -rn TOKEN .` from the plane root printed every vault file**
([#474](https://github.com/diazoxide/charter/issues/474)). Both vault guards decided on the
text of the operand, so an operand that *contains* the vault directory without naming it
walked past them — on the Bash route and on `Grep(path=".")` alike. This was a documented
trade rather than an oversight, and the trade is revisited on purpose: denying every broad
search is untenable, but denying the ones that really would walk into charter's own state
is not the same thing.

The new predicate asks whether the walk **reaches** that directory. The operand is resolved
against the shell's directory and compared by ancestry, and the entries it is compared
against are read off the filesystem — so `.`, `..`, `../..`, an absolute path and a path
through a symlinked parent are one question rather than five spellings, and a plane whose
`$CHARTER_HOME` puts its vaults somewhere no pattern can spell is covered by the same code.
It fires only when those entries exist and hold something, so a fresh plane never sees it.
And the denial names the exclusion that fixes it — `grep -rn --exclude-dir=.charter …`,
`rg --glob '!.charter' …` — which the guard reads, and which the tests execute, because a
guard that refuses the command it recommends is one people learn to route around.

**One route this deliberately did not chase**, filed instead:
[#496](https://github.com/diazoxide/charter/issues/496). `GIT_DIR=<plane>/.git git checkout
feature` is refused now; `export GIT_DIR=<plane>/.git && git checkout feature` is not,
because the shared walk models exactly one shell effect that crosses segments — a `cd` —
and env assignments are read only where they sit on the invocation itself. "Track the
environment across segments" is a larger claim than "track the working directory across
segments", and it belongs with its own evidence rather than inside this one. It is a row in
the guard's corpus, named after the issue, so closing it turns the row red.

**What still walks past, said plainly.** The new predicate knows which programs walk
directories, and that list is shorter than the list of programs that walk: `find . -type f
-exec cat {} +` and `tar cf - .` read the same files and are allowed, exactly as
`base64 .charter/vaults/db.json` always has been. An interpreter's argument is still text
charter does not re-parse, and `_leak_reason` still does not follow a `cd` earlier in the
same command. Each is pinned as behaviour in `tests/test_vault_path_spellings.py`, so a
later doc that claims otherwise fails the suite.

**And one issue closed by deciding rather than by coding.**
[#476](https://github.com/diazoxide/charter/issues/476) asked whether the vault guards
should fold a Windows-style `.charter\vaults\db.json`. They should not, on the prerequisite
the issue itself named: charter's harness does not run on Windows. It builds and drives a
tmux session and writes vaults at `0o600`; there is no Windows CI and no Windows install
path. Folding `\` would buy nothing on a supported host and would deny POSIX filenames that
legitimately contain a backslash, and a platform-*conditional* fold would make the guard's
answer depend on the host — the exact property `re.IGNORECASE` was added to remove. So
`SECURITY.md` and `docs/secrets.md` now say charter's harness targets POSIX where they used
to name Windows as a reason, the package's `Operating System :: OS Independent` classifier
(which was never true) is now `POSIX`, and the decision is recorded next to the test rather
than left as an open question.

Nothing here changes what is allowed in a workspace clone, which is where branch work and
destructive git belong. `git checkout <path>`, `git restore <path>`, the unstage, `--soft`
and a narrowed search all run in the plane root exactly as before.
