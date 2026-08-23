# frame-parity (#385) fix round 1: the gather.scan() worktrees-only-for-si

_2026-08-23 14:04 · persistent_

frame-parity (#385) fix round 1: the gather.scan() worktrees-only-for-single-repo gap (recorded earlier) is now closed -- gather.scan()/_entry() capture a per-repo worktree_count unconditionally (worktree.dirs_for, filesystem-only), and left renders it as a circleN badge. Also fixed in this round: _repo_line/_piece_line used to truncate the WHOLE assembled line once from the right, silently dropping dirty/CI/change markers whenever name+branch alone filled the 22-column pane (true for this project's own branch names, 21-28 chars). Replaced with charter/frame/slots.py's _row(), which budgets each field separately (mirrors statusline._branch_cell_for's 'reserve room for markers first' rule) and drops CI/change/badge whole rather than character-truncating them. Commit 53082f8.
