# Live-testing charter CLI changes from a worktree does NOT work: charter.

_2026-08-13 23:40 · persistent_

Live-testing charter CLI changes from a worktree does NOT work: charter.toml is tracked, so a worktree of charter is its own plane root and sees none of the real plane's workspaces. Use a throwaway plane (charter init in a temp dir) with PYTHONPATH pointed at the worktree instead.
