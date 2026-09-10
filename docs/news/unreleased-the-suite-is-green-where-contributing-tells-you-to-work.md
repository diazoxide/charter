---
version: unreleased
headline: The test suite comes back clean in a workspace clone and in a worktree cut from one — the two places CONTRIBUTING sends you
---

**CONTRIBUTING says "work in a workspace clone, not in the plane root", and doing that
turned four tests red on unmodified code.** `python3 -m unittest` reported `FAILED
(failures=2, errors=2)` out of ~11.9k in a clone under `workspaces/<ws>/`, and the same four
in a worktree cut from one — what `charter wt add <repo> <piece> -w <ws>` builds. They were
green in the plane root and green on CI, so nobody had been looking at them, and a
contributor arriving at four reds could not tell them from their own breakage. Two
implementers hit the same four independently in one day.

They were two unrelated defects that happened to show up together.

**The suite was not pinned to its own checkout.** `tests/_planeguard.py` moves the suite's
committed settings to the tree it was loaded from, so a run asserts against the branch
rather than against whoever's `charter.toml` happens to be on the machine. It asked one
question — `root.tree_of(config.ROOT, <this checkout>)` — which matches a linked worktree of
the plane's own repo and, correctly and by design, nothing else. A workspace clone is a
different repository, and `root.find_root` hops outward through `workspaces/`, so
`config.ROOT` was the outer plane and the two operands were never equal. Nothing was pinned:
the suite read the operator's own file, one case failed loudly against it and another passed
because that file happened to satisfy it. The pin now asks `root.nested_plane_in` as well —
the sibling detector written for exactly this arrangement — and then asks `tree_of` again
about what it found, which lands the clone and any worktree cut from it. Both planes it
moves away from, the clone and the outer one, are folded into the spawn tripwire, so the
refusal gets wider and never narrower.

**The status line's crash guard rendered against whatever plane you had.** Those cases
render against the real plane on purpose and redirect only `config.STATE_DIR` to a tempdir —
and that redirect is what armed a fork, because the background forge refresh keeps its
cooldown lock and its cache there and a fresh tempdir always looks stale. What decided the
outcome was whether the active workspace held any clones: none in the plane root and none on
a CI checkout, seven in a workspace. Where there were clones, the render forked `charter
gl-refresh` at the developer's live plane and the suite's own tripwire refused it. The cases
now stop the spawners they never wanted, and say so.

Both are pinned by tests that ask the question of a fixture instead of of the machine the
suite is running on, since asking the machine is how a defect this shape stays invisible for
a release: one builds a plane, a clone inside its `workspaces/`, and a worktree of that clone
out of plain directories, and one runs the render guard against a plane that has a clone in
it. The trigger for the pin is being cut from a nested *plane*, not a path with `workspaces/`
in it, so a worktree placed anywhere on disk is covered and a charter checkout that has
nothing to do with your plane is left alone.

Nothing to adopt — this is charter's own suite. If you contribute to charter, the clone and
the worktree CONTRIBUTING asks for now come back clean.
