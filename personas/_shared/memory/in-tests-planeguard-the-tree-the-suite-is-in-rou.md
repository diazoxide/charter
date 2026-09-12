# In tests/_planeguard._the_tree_the_suite_is_in, route 2 (root.nested_pla

_2026-09-10 17:26 · persistent_

In tests/_planeguard._the_tree_the_suite_is_in, route 2 (root.nested_plane_in) may overrule route 1 only with the checkout ITSELF carrying its committed marker. nested_plane_in starts at the nearest marker ABOVE the checkout, so for a markerless checkout it finds somebody else's (the clone a plane-worktree sits in, or a worktree of the clone) and the marker check then reads the wrong charter.toml and passes. 'tree_of(nested, here) or (nested if here == nested)' still pinned a markerless plane-worktree placed inside a worktree of the clone (#949 review).
