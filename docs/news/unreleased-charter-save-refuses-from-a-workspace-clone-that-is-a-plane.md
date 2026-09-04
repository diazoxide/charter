---
version: unreleased
headline: '`charter save` from a workspace clone that is itself a plane refuses, instead of committing the outer plane'
---

*"I ran `charter save` in `workspaces/dev/charter`. It committed the operator's `NOTES.txt`
from the plane above me — and none of my own work in the clone."*

## What was measured

A throwaway plane, a real `git clone` of it at `workspaces/dev/charter` (where `charter
clone` puts clones), and the `.gitignore` `charter init` writes. The operator is mid-edit in
the outer plane; the agent's own change sits in the clone. `charter save "agent: unrelated
security fix"`, typed in the clone, on the tree before this change:

```
✓ Committed a581f80 in /…/plane: agent: unrelated security fix  (1 file(s))
 NOTES.txt | 1 +
--- the agent's own work, in the clone ---
?? agent-work.txt
```

Exit 0. Two harms, both silent:

1. the operator's untracked `NOTES.txt` in the **outer** plane, committed under the agent's
   message, on the plane's own branch;
2. the agent's own work — the only thing it asked to save — **not committed at all**, and
   the clone's `HEAD` not moved.

## Why this was not already fixed

`charter clone` puts clones at `workspaces/<ws>/<repo>`, `charter.toml` is a tracked file,
and charter dogfoods on a clone of itself — so `workspaces/dev/charter` is a control plane
nested inside a control plane. `root._outermost` resolves outward there, so `config.ROOT` is
the plane above, and `charter save` is `commit_push(config.ROOT, ["add", "-A"], …)`.

**That resolution is right and has not changed.** The outer plane is the one holding the
vault, the personas and the memory; #200 measured what happens when identity lands in the
inner one instead. What was wrong was `save` inheriting an answer meant for identity and
using it to pick a *working tree*.

The worktree fix that shipped alongside this (`charter save` refuses from a linked worktree)
could not cover it. Its detector reads the `.git` **file** git writes into a linked worktree;
a clone has a real `.git` **directory**, so it correctly answers "not a worktree" and must
keep doing so. This is a second detector, `root.nested_plane_in`, asking the `workspaces/`
question — not a widening of the first.

## What happens now

```
✗ Refusing to stage all of /…/plane — you are standing in /…/plane/workspaces/dev/charter,
  a control plane of its own under that plane's workspaces/. charter resolves outward to the
  plane holding the vault, so committing every change there would take that tree's
  uncommitted work under your message, and leave your own work here unsaved.
•   your own work:   git -C /…/workspaces/dev/charter add -A && git -C /…/workspaces/dev/charter commit
•   the plane's own: run `charter save` from /…/plane
•   you really do mean this plane: CHARTER_ROOT=/…/workspaces/dev/charter charter save
```

Charter refuses and names **both** trees, rather than silently picking one. Committing the
inner plane instead would have made `save` disagree with every other command about which
plane it acts on — `persona remember`, the vault, the dispatch tally and the workspace
manifest all follow `config.ROOT` outward — and split the plane in two.

## What still works from inside a clone

Only the **unbounded** stage refuses. `charter save` is the sole caller that stages the whole
tree; reactive memory, the dispatch tally, the workspace manifest and `version bump --push`
all name plane-state files, and those keep committing to the plane from anywhere. Working
inside a clone is the ordinary way work happens on a charter plane, and every agent doing it
writes memory — refusing that would have been a larger harm than the one being fixed.

`CHARTER_ROOT=<the clone> charter save` commits the clone, as the refusal says. The guard
asks whether you are standing in a plane nested inside *the plane being committed*; under
that override, the plane being committed is the one you are standing in.
