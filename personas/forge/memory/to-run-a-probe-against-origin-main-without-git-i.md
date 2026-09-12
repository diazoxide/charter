# To run a probe against origin/main without git in any in-use worktree: g

_2026-09-10 22:57 · persistent_

To run a probe against origin/main without git in any in-use worktree: git fetch in the clone, then git archive origin/main piped to tar -x -C into scratch, and run with PYTHONPATH set to that export; print charter.__file__ first to prove the export (not the uv-installed charter) was imported. tests/_isolation.PersonaIso works from the export.
