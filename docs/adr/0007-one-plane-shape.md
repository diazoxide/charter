# One plane shape — and the obvious reason for it is the wrong one

charter had two plane shapes. **fleet**: the plane is its own directory and a workspace
holds clones. **embedded**: charter serves the codebase it sits inside, and a workspace
holds worktrees of it. The embedded shape is removed; fleet is the only shape, and the
`[plane]` section and its `shape` key are deleted rather than kept and validated.

The justification is **one code path instead of two**. Every function that asked "which
shape am I?" was a place the two could drift, and only one of them was ever exercised by
any given plane — so the other was carried on trust.

## What this decision is NOT

It is not a fix for workspaces thrashing each other's git branches, which is the symptom
that prompted it. Recording that plainly, because the next person to hit branch-thrashing
in a fleet plane will otherwise conclude this removal failed, or was never done.

Embedded workspaces already had their own worktrees — see the `own_tree` docstring, which
records that exact bug and that exact fix, predating this decision. What remained were two
holes underneath it:

1. the `default` workspace owned the plane root outright, so it isolated nothing; and
2. selecting a workspace never moved the session into its tree — charter printed
   `cd … && claude` and trusted the caller.

Removing the shape closes the first hole outright: `own_tree` is what made `default` the
plane root, and a fleet plane's `repo_trees` is its clones and nothing else — `root_tree()`
returns `None` in any shape but embedded, verified by running it. So after this change
charter never *presents* the plane root as a tree you work in.

The second hole survives, and it is the one that bites. Nothing **stops** a session sitting
in the plane root and running git there, whatever charter lists; the plane root is a real
git repo and a real directory. Not presenting it is not the same as preventing it, which is
why this ships alongside ADR 0008.

## Consequences

charter develops itself through a clone of itself: the self-exclude in its own
`charter.toml` existed only because "the repo is already here, as the plane's root tree",
which stops being true.

The solo-user path gains a step. `own_tree` promised that one person with one repo could
`charter init` and carry on working in that repo; now they must clone it into a workspace.
`charter init` inside a git repo offers to make that clone immediately, so the step is met
once at setup rather than discovered later as "where did my code go?".

No backward compatibility: `shape = "embedded"` in an existing `charter.toml` becomes an
ignored unknown key. There is no migration, no refusal, and no deprecation window, and
materialised worktrees are left exactly where they are — they remain valid git worktrees
whether or not charter manages them.
