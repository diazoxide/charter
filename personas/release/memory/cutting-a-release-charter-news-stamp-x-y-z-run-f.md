# Cutting a release: 'charter news stamp <X.Y.Z>' run from the release wor

_2026-09-05 21:00 · persistent_

Cutting a release: 'charter news stamp <X.Y.Z>' run from the release worktree is REFUSED by the installed CLI ('news entries are stamped in the repo, and this is not a charter checkout'). Run it as 'python3 -m charter news stamp <X.Y.Z>' from inside the worktree instead. Same for 'python3 -m charter news --for <X.Y.Z>' when checking the gate and the lead ordering pre-tag.
