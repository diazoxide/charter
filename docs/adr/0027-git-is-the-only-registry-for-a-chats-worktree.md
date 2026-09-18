# Git is the only registry for a chat's worktree

M1.4 gives a chat that writes to a repo its own git worktree, cut and listed by the Rust
core. Four of its choices sit against rules written elsewhere in this repo, and each will be
re-proposed by whoever picks up the next milestone. They are written down here, together,
because three of them share one reason.

**Charter shells out to the git binary, records nothing of its own about a worktree, ships
worktrees that carry no harness layer, and narrows M1.4's differential test to git's state
rather than the resulting tree.**

## Git through the binary, not a library

The alternatives are libgit2 through `git2`, and gitoxide. Neither is chosen.

`git worktree` is not one operation; it is a registry under `.git/worktrees`, a set of
gitlink files pointing both ways, a `prunable` state for a tree whose directory went away,
and a locking protocol between concurrent `add`s. The binary is the definition of that
behaviour. libgit2 reimplements it and diverges in the corners that matter here — which is
the wrong kind of difference to discover from a bug report about a worktree charter created
and git cannot see. Gitoxide has no `worktree add` at all.

Priority 1 is development experience, and `git2` costs a C toolchain on every machine and in
CI, plus a libgit2 version to track. Priority 2 is standard practice, and the standard
practice for driving git is git.

**The objection is parsing.** `git worktree list --porcelain` has to be read, and this repo's
decision 12 says nothing parses output to decide anything. That decision is about *harnesses*
— programs whose output is prose written for a person, whose shape is not promised and
changes when a vendor feels like it. `--porcelain` is the opposite: git documents it as a
machine format and holds it stable across releases precisely so that tools may depend on it.
Python charter has parsed it since the feature existed. Reading a documented machine format
is not inferring a fact from prose.

What is given up is real and should be named: every call is a process spawn, roughly a
millisecond, and errors arrive as exit codes plus English on stderr. Charter never reads that
English to decide anything. Where a call fails and the cause matters — two chats racing for
one piece — charter re-reads the repository and names a cause it established by looking,
which is the only kind ADR 0009 permits.

## Nothing records a worktree except git

Python charter's `worktree.py` opens with the rule and the reason: *"Git is the only registry.
Nothing here writes state."* A worktree made by hand with plain git is visible to charter, and
one removed by hand cannot leave charter reporting a tree that is not there. The alternative
is a marker that can disagree with reality, and this project has been bitten by that shape
before.

Python then breaks its own rule in one place: it records `claimed` / `done` / `abandoned` per
piece so that `charter wt history` can show pieces that no longer exist. The Rust core does
not carry that. History is a genuine want, but it is a plane-format write, and a plane-format
write needs its own differential test and its own entry in `docs/plane-format.md`. Riding it
in on a milestone about worktrees is how a second registry gets created by accident.

**The base branch is the hard case.** Merging back targets the branch a piece was cut from
(see the design doc), and something must remember which that was. Inferring it later does not
work: `git merge-base` yields a commit, and a commit that two branches both reach names
neither. So it is recorded — in git, as `branch.<branch>.charterBase` in the clone's own
config, keyed on the branch the worktree is really on rather than on the piece name, which
`--branch` makes a different string.

That key needs no `extensions.worktreeConfig`, which is the reason it is not
`git config --worktree`: measured on git 2.50.1, `--worktree` fails with exit 128 the moment
a repository has a linked worktree unless that extension is enabled, and enabling it mutates
the operator's clone repository-wide to store one string. A `branch.*` key costs nothing, git
ignores keys it does not know, and git drops the section when the branch is deleted — so the
record's lifetime is the branch's lifetime with nobody maintaining it.

A worktree with no such key is one charter did not cut. That reads as "charter does not know
the base", which is true, rather than as a default that would be a guess.

## A worktree charter cuts carries no harness layer, and says so

This is the one accepted gap, and it is a guard hole, so it is stated plainly rather than
left to be discovered.

Python charter writes a *guest layer* into every tree it owns, including every piece:
`workspace.wire_guest` materialises the harness plugin, `$CHARTER_HARNESS`, the persona's
agents and the plane's ask/deny rules, and hides them through a managed block in
`.git/info/exclude`. A session started in a tree without it runs with none of charter's
guards. Charter #951 is the bug report for exactly this, from when pieces were missed.

The Rust core does not port that in M1.4. It is a four-hundred-line subsystem with its own
ownership model — which paths charter may claim, which are co-written, what happens when the
block cannot be written — and folding it into this milestone would put it in the same review
as the containment work, where neither gets read properly.

Decision 14 forbids calling Python and forbids scaffolding written only to be deleted. This
is neither: no Python is called, and nothing here is scaffolding. The gap is made visible
instead, in two ways that cost nothing to remove later:

- A chat's row reads `unwired` when its worktree has no `.charter-generated` marker. That is a
  reading of the tree, not a memory of what charter did — so a worktree Python cut reads as
  wired, and a worktree that *becomes* wired by a later `charter reinit` stops showing the
  label on its own, with no code to delete.
- A **persona cannot be attached to a chat in an unwired worktree.** A persona whose agents
  and vault rules are silently absent is worse than no persona: the operator believes a
  charter is loaded and it is not. The refusal names the gap at the moment it would matter.

Until the layer is ported, a chat in a charter-cut worktree is a chat without charter's
guards. That is the bound: it is visible on every row, it blocks the one feature that would
mislead, and it is a todo in the `ide` workspace rather than a line in a design document
nobody re-reads.

## The differential compares git's state, not the tree

Decision 15 requires every ported module to pass a differential test: the same fixture plane
and the same input give the same output and the same resulting plane in both implementations.
For worktrees, a tree comparison cannot be that test.

Two reasons, and the second is the one that matters. Python writes the guest layer and the
piece history; Rust deliberately writes neither, so a byte-for-byte tree comparison fails on a
*correct* implementation — it would be measuring the two divergences this ADR just decided.
And a git repository does not compare byte for byte against itself: reflog entries carry
timestamps, the index carries mtimes, and objects are loose or packed depending on when git
last felt like repacking.

Narrowing to an exclusion list is the wrong repair. An exclusion list grows every time git
changes an internal detail, and a comparison whose exclusions nobody reads is a green light
with no bulb behind it.

So the compared surface is git's own answer about its own state: `git worktree list
--porcelain`, plus the branch and the commit each tree sits at, captured from both
implementations against a throwaway repository built with pinned author and dates. Both must
produce the same registration, the same branch, at the same commit. `tests/differential/run.py`
learns a second scenario kind to do it — comparing captured command output instead of trees —
which is reusable for every later module whose effect is not a file.

This keeps decision 15's substance. What is given up is stated rather than implied: the
differential no longer covers what either implementation writes *around* the worktree, because
that is exactly where the two are agreed to differ.

## What this rules out

- A git library, unless someone first shows a behaviour the binary cannot give.
- A charter-side file listing worktrees, for any reason, including performance.
- Inferring a piece's base branch instead of recording it.
- A silent unwired worktree: if the label or the persona refusal is removed, the layer is
  ported first.
- Treating M1.4's differential as covering the resulting plane. It covers git.
