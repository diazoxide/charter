# tests/test_plane_spawn_guard.py NoCharterEscapesThroughTheExecFamily.tes

_2026-09-11 08:07 · persistent_

tests/test_plane_spawn_guard.py NoCharterEscapesThroughTheExecFamily.test_every_exec_in_the_tree_is_one_that_cannot_become_charter scans the WHOLE tree statically, tests/ included, for exec-family calls (os.execvp and kin). Every call site must appear in that module's KNOWN list. Measured on PR 966 round 1 (2026-09-11): a tests-only commit that added one os.execvp (attaching a real tmux client) passed its 538 focused tests locally, then failed CI deterministically on every Python with {'tests/<file>.py:execvp': <line>} != {}. So run the FULL suite for any commit that adds an exec or spawn call, even a tests-only one; a focused run cannot see this guard. Admitting a new site means a KNOWN entry with a true reason it cannot become charter, never loosening the check.
