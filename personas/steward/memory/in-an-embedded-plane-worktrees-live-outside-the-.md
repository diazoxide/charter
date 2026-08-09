# In an embedded plane, worktrees live OUTSIDE the codebase. Inside, each

_2026-08-09 22:54 · persistent_

In an embedded plane, worktrees live OUTSIDE the codebase. Inside, each is a full second copy of the source that every root-level glob walks — measured here at 214 discoverable test files of which 142 were duplicates. .gitignore hides that from git and from nothing else.
