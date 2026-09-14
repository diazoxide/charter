---
version: unreleased
headline: `charter doctor` names a memory directory, a workspace or a clone it could not look at, where it used to read it as empty, absent or fine
---

Four places in `charter doctor` took "could not look" for an answer.

- **`workspaces/` at mode 666** (readable, not searchable). On Python 3.11–3.13 every row
  that lists workspaces said `not checked`. On 3.14 the workspaces silently dropped out of
  those rows, and the rows went green:

  ```
    ✓  memory indexes    1 base(s) consistent
    ✓  workspace clones  no clones in any workspace — nothing to check
  ```

- **A `memory/` directory charter cannot list.** At mode 000 the row said the index was a
  defect in a committed file and told you to "replace the link", over a directory that is no
  link. At mode 333 it reported the memories it could not list as dangling.
- **A `memory/` that is a symlink to nothing outside the plane** was skipped as an absent
  base, so the check that refuses exactly that link never ran.
- **A clone whose `.git` charter cannot stat** was left out of `workspace clones` without a
  word, though `charter git-policy` already named it.

Now each row checks what it can read and names the rest, with what clears it, on every
Python version:

```
  !  memory indexes    1 base(s) consistent; workspaces/alpha, workspaces/beta cannot be checked
        → workspaces/alpha cannot be checked — restoring read access to it clears this; …
  !  workspace clones  1 clone(s) across 2 of 2 workspace(s), none behind; workspaces/beta/web cannot be checked
        → workspaces/beta/web cannot be checked — restoring read access to it clears this.
```

The `workspace layer` and `changes` rows name an unreadable workspace the same way. A
`memory/` linked out of the plane to nothing is reported as an index charter will not touch.

Nothing to adopt ([#1043](https://github.com/diazoxide/charter/issues/1043)).
