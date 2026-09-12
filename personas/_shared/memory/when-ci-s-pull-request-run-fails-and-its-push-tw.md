# WHEN CI'S pull_request RUN FAILS AND ITS push TWIN ON THE SAME SHA PASSE

_2026-09-10 18:22 · persistent_

WHEN CI'S pull_request RUN FAILS AND ITS push TWIN ON THE SAME SHA PASSES, main MOVED UNDER THE BRANCH: pull_request tests the merge ref. Measured on #939: main's #943 made SessionStart fork a version check, so the branch's SessionStart tests were green alone and seven errors on the merge. Reproduce with 'git merge-tree --write-tree HEAD origin/main' then 'git archive <tree> | tar -x -C <scratch>' — it touches no branch, ref or worktree. Never then cp branch files over that merged tree: it erases main's side of the three-way merge, and on #939 that manufactured a second, fake failure (main had stubbed the fork in a file the cp overwrote). Separately: never clean up processes with pkill -f by name or pattern — on #939 'pkill -f tools/sweep.py$' likely killed another agent's sweep. Record $! for what you start and wait on or kill only that PID.
