---
version: unreleased
headline: charter save tells a clean tree from one it could not read, and refuses the second
---

On the operator's own plane, not hypothesised:

```
$ git status --porcelain | wc -l
      13
$ charter save
• Nothing to save — the control-plane working tree is clean.
```

The cause was a stale `.git/index.lock` — zero bytes, twenty-three hours old, no process
holding it, left behind by a git that crashed the day before. The operator removed it by
hand after checking `ps` and the file's mtime, and the identical command on the identical
tree committed twenty files.

## The lock is invisible to the question charter asked last

Measured against git 2.50.1, in a repository with one modified tracked file and one
untracked file, with a zero-byte `index.lock` planted in it:

| | |
|---|---|
| `git status --porcelain` | rc 0, both files listed — reading the tree needs no lock |
| `git add -A` | rc 128, `Unable to create '….git/index.lock': File exists.` |
| `git diff --cached --quiet` | rc 0 — nothing was staged, so there is no difference |

`commit_push` discarded the add's exit status. The probe below it then answered *no
difference*, because there was none: nothing had been written to the index. **"The tree is
clean" and "charter could not tell" were the same value**, and charter printed the first.

That asymmetry is why this rendered as a confident sentence rather than as an error, and
why it survived. It is also why this instance is worse than its siblings: every other
version of this shape in charter degrades to *nothing to show* — a missing gauge, an empty
row, a cache read as cold. This one degrades to a claim about durability that is false, and
the operator's next act is to stop worrying about the work. It becomes data loss the moment
anything afterwards assumes the save happened.

## Three outcomes, and charter says which

`charter save` now answers in three states instead of two. The tree is clean; the tree has
changes; or charter could not determine the state — in which case it **refuses**, with rc 1:

```
✗ Refusing to save — charter could not read the state of /Users/…/charter, so it
  cannot tell an unchanged tree from an unreadable one.
✗   `git add -A` exited 128 — fatal: Unable to create '….git/index.lock': File exists.
•   /Users/…/charter/.git/index.lock — 0 byte(s), 23h old; a git process crashed here
    and left it behind.
•   who holds it:  ps -eo pid,lstart,command | grep '[g]it'
•   nobody does:   rm -f /Users/…/charter/.git/index.lock   (charter never removes a
    lock — a held one is real)
```

The age is in there because it is the fact that changes what the operator does. A lock
seconds old is somebody else's `git commit` and the answer is to wait; zero bytes and hours
old is a corpse and the answer is `rm`. A message that said only "a lock is present" would
send both of them to the same place. Zero bytes is what separates the two: git creates the
lock, writes the new index into it, and renames it over the old one, so a lock that never
grew is one whose writer died before it wrote anything.

**charter reports a lock and never clears one.** A lock held by a live git is real, and
nothing inside a single command can reliably tell a held lock from an abandoned one — the
holder may be a `git commit` with an editor open, a `git gc` a second from finishing, or
another machine's view of the same mount. Removing it would corrupt an index that was about
to be written, and a program that clears locks teaches its operator that locks do not mean
anything. Charter states the path, the size and the age, and leaves the `rm` to the person
who can run `ps` first.

## `doctor` notices it before a save runs into it

A new `index lock` row. A lock is not a fault — git takes one for every write to the index
— so a fresh one is stated on a green row and nothing else. Zero bytes and older than
fifteen minutes is a WARN naming the file, its age and the two commands, because that one
will refuse every `charter save` and every `git add` in the plane until somebody clears it.
The operator's lock sat there for twenty-three hours, through however many sessions, and the
first thing to look at it was the save it broke.

## The same conflation, everywhere it decided something

A grep for every place charter read `git status` found eleven such readings across seven
modules, and they fail in **pairs** — which is how a defect of this shape survives being
noticed. `charter/gitstate.py` is now the one place that asks, and it answers in three
states rather than two. The sites that made a decision on the old two-state answer:

- **`workspace remove`** — what `_work_at_risk` and `_worktrees_at_risk` come back empty of
  is what gets handed to `shutil.rmtree`. A clone or worktree charter could not read is now
  work at risk, which is what it always was.
- **`workspace snapshot`** — an unreadable clone blocks. The manifest is a claim, made to
  another engineer on another machine, that the recorded branch captures reality.
- **`charter sync`** — the empty answer used to *authorise* the `git merge --ff-only` two
  lines below it. An unread tree is now skipped, on the same terms as one with work in it.
- **`worktree remove`** — refuses, in the words its `unique_commits` guard has used since
  #104. `worktree.is_dirty` returned `bool`, which is why fixing the call sites alone could
  not have worked; it is `worktree.dirt` now, and it answers in three.
- **`persona memory-sync`** and the **session-start memory nudge** — both halves of "is
  memory unsynced" answered *no* on the same failure at the same moment: one printed a green
  tick, the other stayed silent. Both now say they could not look. A plane with no git
  repository at all is exempt and unchanged — that is a definite answer, and `charter init`
  in a fresh directory not running `git init` is the README's own 60-second path.
- **`doctor`'s plane-root row** — the one rc-blind git call in a function that checks every
  other one it makes. "not checked", never a tick.
- **the deletion sweep** — `dirty_files` passed `check=False` to a helper that returns
  stdout only, so an empty listing built the sandbox from committed HEAD alone and the whole
  run reported findings against a tree the operator was not looking at. It raises
  `NoSandbox` now, which is exactly that category and lives twenty lines above it.

Two display sites got the third word for one line's worth of change each — `charter status`
and `worktree list` print `unknown` rather than `clean` for a tree nobody could read.
`statusline._run_state` is the one that still conflates them, and it is left alone on
purpose: it is a glyph, it is behind a five-second cache three renderers share, and changing
what that cache stores is a larger piece of work than this one.

This is the third of these in a row, and each was fixed by giving the failure a category of
its own rather than a shared default: `NoSandbox` (#905/#909) says *the machine*, not *the
branch*; `config.PLANE_REFUSAL` (#913/#914) says *refused*, not *carry on with defaults*;
and `gitstate.TreeState` says *charter could not tell*, not *there is nothing there*.
