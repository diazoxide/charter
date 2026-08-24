---
version: unreleased
headline: A `git reset --hard` in the plane root that would delete unpushed commits is now refused
---

0.51.0 taught `charter doctor` and the status line to name the hazard: a memory commit sitting
on a local `main` that never reached the remote, and *"push it before anything runs `git reset
--hard origin/main` here, which would delete it silently."* That is a warning. Eleven commits
were lost to exactly that command, in a session that had the warning in front of it.

Naming a hazard is not preventing it, and the plane root already had a guard one subcommand
away. `checkout` and `switch` were refused there; `reset` was left out for want of evidence
about which commands count. The evidence arrived.

**What is refused.** A `git reset --hard` — or `--merge`, or `--keep` — in the plane root,
when charter can see that it would take commits off the branch that **no remote has a copy
of**. The denial says how many, points at `git -C <plane> log --oneline '@{upstream}..HEAD'`
so you can read what would have gone, and names `charter save` as the way to keep them.

**It clears itself.** The condition is measured, not remembered: push the commits and the same
`git reset --hard origin/main` runs, because there is nothing left to lose.

**Almost every `reset` is untouched**, which is the whole design. `git reset HEAD -- <path>` is
an unstage and is the most common `reset` an agent types — it moves no commit and is allowed.
`--soft` and `--mixed` take the branch off the same commits but leave every byte in the working
tree, so the next `charter save` re-commits and re-pushes that content by itself; they are
allowed too. `git reset --hard` with no ref throws away uncommitted work and no commits at all
— a different hazard, one `doctor` already counts — and is allowed. A reset over commits that
are already on the remote is one fetch from recovery, so it is ordinary work and the guard
stays quiet through it. On a plane with no tracking branch charter cannot say what is
published, so it says nothing rather than guessing.

**A hole in the older guard closed on the way past.** Both plane-root guards read "the first
non-flag argument" as the subcommand, so `git -c commit.gpgsign=false checkout feature` in the
root presented `commit.gpgsign=false` as its subcommand and walked through. That is the form
this repo's own commit convention teaches. The two guards now share one walk over the command
— the `cd` tracking, the `-C` retargeting and the global-option handling in one place, because
a guard's blind spot looks exactly like a guard that is present and never fires.

Nothing to adopt: upgrading is the whole of it.
