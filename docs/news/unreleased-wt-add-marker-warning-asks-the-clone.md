---
version: unreleased
headline: `charter wt add` stops telling every guest repo to `git add charter.toml`
---

A worktree of any repo a workspace had cloned printed this, on a plane that was set up
correctly:

```
!   charter.toml is not committed, so it is absent from this worktree. charter
    resolves the plane from the repo it was cut from, but teammates cloning
    this repo get no control plane — commit it: git add charter.toml
```

The check asked whether the plane had a `charter.toml`, which every plane has. It never
looked at the repo the worktree was cut from, and that repo, a project the workspace
cloned, has no `charter.toml` to add, so the command it named failed wherever you ran it.

Now the line asks the repo the worktree was cut from. It prints only when that clone holds
a `charter.toml` its HEAD does not commit, and it names the clone in the command:
`git -C workspaces/<ws>/<repo> add charter.toml`. A repo with no `charter.toml` of its own
gets no line. Neither does a branch picked with `--branch` that was cut before a committed
`charter.toml` landed: the file is committed, so "is not committed" would be false.
