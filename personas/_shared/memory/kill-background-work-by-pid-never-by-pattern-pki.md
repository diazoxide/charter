# KILL BACKGROUND WORK BY PID, NEVER BY PATTERN — pkill -f HITS OTHER AGEN

_2026-09-03 01:57 · persistent_

KILL BACKGROUND WORK BY PID, NEVER BY PATTERN — pkill -f HITS OTHER AGENTS. Measured 2026-09-03: an agent ran pkill -f 'unittest discover -s tests$' to stop its own suite and killed a CONCURRENT agent's full-suite run on the same machine. The victim restarted on its own, but the killer had no way to know it had done damage and the victim had no way to know why its run vanished. Several agents run the identical command by construction — every brief in this repo tells them to verify the way CI does, which is 'python -m unittest discover -s tests'. Rule: capture the PID when you start the process ($! for a background job) and kill that; if you must search, scope it to your own worktree path and PRINT what you matched before killing. Related: [[never-run-git-commands-in-a-clone-a-dispatched-a]] — same class, one layer down: shared machine, not shared repo.
