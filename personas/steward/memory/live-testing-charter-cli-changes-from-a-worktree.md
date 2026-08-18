# Live-testing charter CLI changes from a worktree does NOT work: charter.

_2026-08-13 23:40 · persistent_

Live-testing charter CLI changes from a worktree does NOT work: charter.toml is tracked, so a worktree of charter is its own plane root and sees none of the real plane's workspaces. Use a throwaway plane (charter init in a temp dir) with PYTHONPATH pointed at the worktree instead.

CORRECTION (2026-08-17): "PYTHONPATH pointed at the worktree" is NOT enough on its own, and fails SILENTLY — it runs the old code and reports a clean pass. `python3 -m charter` puts the CWD first on sys.path, ahead of PYTHONPATH, so running it from inside any other charter checkout (the clone, the plane root) imports THAT copy. Cost a false "the fix does not work" while all three surfaces were in fact correct.

Always assert which package actually loaded before trusting a live check:
`python3 -c "import charter; print(charter.__file__)"`
Then either cd INTO the worktree (so cwd is the copy you want) and set CHARTER_ROOT to aim the plane elsewhere, or use a cwd that is not a charter checkout at all.
