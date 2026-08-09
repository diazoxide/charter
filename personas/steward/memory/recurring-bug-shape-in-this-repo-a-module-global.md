# Recurring bug shape in this repo: a module global read where the file on

_2026-08-09 22:54 · persistent_

Recurring bug shape in this repo: a module global read where the file on disk was meant. WORKTREES_ROOT resolved at import from the real repo and leaked into tests; cmd_reinit read config.PLANE_SHAPE while drift read charter.toml. If two paths answer the same question, call the same function.
