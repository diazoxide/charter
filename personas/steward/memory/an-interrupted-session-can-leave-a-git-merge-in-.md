# An interrupted session can leave a git MERGE in progress: MERGE_HEAD set

_2026-09-11 21:52 · persistent_

An interrupted session can leave a git MERGE in progress: MERGE_HEAD set, conflicts resolved in the working tree, nothing committed. 'git log' then shows the OLD head and 'git merge-base --is-ancestor <merged-sha> HEAD' says NO, which reads exactly like 'the merge never happened' — but the resolution is sitting right there unstaged/UU. Before re-merging or re-doing work, check: git rev-parse -q --verify MERGE_HEAD, and git status --short for UU rows. Also: local origin/main can be stale, so fetch before concluding a merge-base is wrong.
