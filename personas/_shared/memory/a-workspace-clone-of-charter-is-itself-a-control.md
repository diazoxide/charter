# A workspace clone of charter is itself a control plane, so a concurrent

_2026-08-22 17:59 · persistent_

A workspace clone of charter is itself a control plane, so a concurrent 'charter save' can check out main and commit in that clone while you are working in it — it happened mid-task and moved HEAD off the feature branch. Assert branch + HEAD identity around any long operation in such a clone, and re-check after 'git stash pop'.


## CORRECTION 2026-09-01 — the mechanism named here is wrong

This memory says a concurrent `charter save` "can check out main and commit in that
clone". **`charter/planegit.py` never runs `checkout`, `switch` or `reset`** — grepped
on `main` at 9c0d0a9: zero occurrences. `_land_via_branch`'s own docstring says it never
moves HEAD, and `git rebase FETCH_HEAD` was measured both ways: with the tree dirty (the
state this incident describes) it **refuses outright** and HEAD does not move; clean, it
rebases and stays on the branch.

**The command that really checks out a branch in a clone you are working in is
`charter workspace restore`** (`commands_workspace.py`, around :602 and :640), not
`charter save`.

**The harm this memory records is real, and #806 found its actual mechanism:** `charter
save` typed inside a linked worktree resolves `config.ROOT` to the parent clone and runs
`git -C <plane> add -A`, committing the operator's uncommitted work under the agent's
message while not saving the agent's own. Fixed on `main` by #808 — `commit_push` now
refuses an unbounded add from a worktree, and every save names the tree it committed.

**Still reachable, and out of #806's scope:** the same three harms from a workspace
*clone* that is itself a plane (`workspaces/<ws>/charter`), because `_outermost` resolves
outward by #140's deliberate choice. `tree_of` is narrow by design and does not cover it.

Why the correction matters more than the detail: I cited this memory in a brief as
evidence for a `rebase` hazard, and an agent measured it and found the citation did not
support the claim. A memory that names the wrong mechanism sends the next reader to the
wrong file with confidence.
