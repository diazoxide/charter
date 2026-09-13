---
version: unreleased
headline: Wiring a worktree no longer hides an untracked file of yours in its clone, and a clone whose worktrees git cannot list is named
---

A clone and its linked worktrees read one `.git/info/exclude`. Charter hides the files it writes
by listing each path there, so when it wired a piece `charter wt add` cut, the line for the
piece's `.claude/settings.json` hid that path in the clone too. If the clone held an untracked
`.claude/settings.json` of your own, one charter never wrote, it disappeared from the clone's
`git status`. Nothing was deleted, but work you had not committed stopped showing, and that is
how uncommitted work gets lost.

Now charter checks every checkout that reads the exclude before it adds a line. If one of them
holds an untracked file at that path that charter did not write, charter leaves the line out:

- Your file stays in the clone's `git status`.
- The piece still gets charter's file, so the plane's ask/deny rules are in force there. That file
  shows in the piece's `git status`.
- `charter wt add`, `charter workspace reinit` and `doctor`'s `workspace layer` row name your file,
  and `doctor` marks the piece's `.git/info/exclude` as `unhidden`. Commit your file or move it,
  and the next `charter workspace reinit` hides charter's.
- Until then `charter wt remove` counts charter's file as uncommitted work in the piece and needs
  `--force`.

A file charter wrote still counts as charter's in any checkout, and only a record charter trusts
says so. A `.charter-generated` git tracks does not. A file of yours that git already ignores, or
one that is committed, does not stop the line, because the line hides nothing more of yours.

This covers only lines charter is about to add. A line already in the block stays, because git
reports the path as ignored and cannot say whose file it hides.

Charter also finds a workspace's worktrees by asking git. When git could not list them for a clone
(an unreadable `.git/worktrees`, a git that fails or cannot run), those worktrees got no layer and
no repair, and nothing said so. `doctor` now shows `<clone>/.git/worktrees (unlisted)`, and
`reinit` and `charter clone` name the clone. All three say what clears it: restoring read access,
or fixing a symlink loop.
