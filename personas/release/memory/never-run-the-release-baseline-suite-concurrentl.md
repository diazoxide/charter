# Never run the release baseline suite concurrently with the version bump

_2026-08-21 10:41 · persistent_

Never run the release baseline suite concurrently with the version bump edit. Cutting 0.47.1 I started 'python3 -m unittest discover -s tests' in the background on main and edited the four version files while it ran: it read plugin.json at 0.47.1 while charter.__init__ was still cached at 0.47.0 and reported FAILED (failures=5) in TestVersionsMoveInLockstep — five spurious failures that look exactly like a real drift bug. Run the suite AFTER the bump + stamp, on the release branch, and clear __pycache__ first (0.47.0 -> 0.47.1 is the same byte length, so the mtime/size .pyc check can reuse stale bytecode). Also capture the exit code explicitly: piping the run to 'tail' makes the shell report the pipeline's rc=0 even when the suite FAILED.
