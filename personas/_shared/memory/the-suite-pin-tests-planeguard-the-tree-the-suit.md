# The suite pin (tests/_planeguard._the_tree_the_suite_is_in, #944) must h

_2026-09-10 16:53 · persistent_

The suite pin (tests/_planeguard._the_tree_the_suite_is_in, #944) must hold in three places: a worktree of the plane (root.tree_of), a workspace clone and a worktree cut from one (root.nested_plane_in, then tree_of again). Assert it against a fixture built from plain dirs (a worktree = a .git FILE naming its main tree), never against where the suite happens to run — the plane root and CI have no clones, so a machine-asking case stays green there while clones go red.
