# tests/_planeguard's runtime spawn guard does NOT protect a Popen of a ba

_2026-09-11 10:41 · persistent_

tests/_planeguard's runtime spawn guard does NOT protect a Popen of a bare non-interpreter program: _cmd_launches_charter resolves a bare name through PATH only when the arguments reach an interpreter (-c, -m or a .py argument), so Popen(['tmux', 'attach', ...]) against a PATH 'tmux' shim that imports charter is not refused (measured by the PR #966 round-2 re-review, 2026-09-11). What covers such a call is tests/test_plane_spawn_guard's STATIC exec-family guard. In a test, launch the exact binary under test by absolute path (shutil.which once in setUp) and never claim the runtime guard covers it. forge is filing the gap separately.
