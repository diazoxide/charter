# frame-parity (#385): gather.scan()'s data["worktrees"] is populated ONLY

_2026-08-23 13:36 · persistent_

frame-parity (#385): gather.scan()'s data["worktrees"] is populated ONLY when a workspace resolves to exactly one repo (mirrors statusline._detail_worktrees's single-repo rule) — a multi-repo workspace's per-repo piece count/list is not in the gather cache at all (statusline._repo_rows gets it via a live worktree.dirs_for() call per repo, never folded into gather). Flagged in task-3-report.md rather than patched around in the left panel renderer; needs a decision before task 4/5 or any future panel assumes piece data is always there.
