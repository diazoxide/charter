# commands_workspace.py's cmd_workspace_remove/cmd_workspace_rename (PR #9

_2026-09-10 15:18 · persistent_

commands_workspace.py's cmd_workspace_remove/cmd_workspace_rename (PR #939 head 4d98515) reset the active workspace only when workspace.source() in ('session','active-file') — but workspace.source() (workspace.py:598) never returns 'active-file' for any rung; only 'session' is reachable there. So the reset already misses 'frame' (a framed chat), and 'active-file' in that tuple is currently dead. Verified by reading both files at the PR head via gh api contents, not the checked-out worktree.
