# charter doctor cannot tell a hook run from a person: the SessionStart ho

_2026-09-11 13:07 · persistent_

charter doctor cannot tell a hook run from a person: the SessionStart hook in hooks/hooks.json runs the same argv a person types, and a tty check misreads --json or a pipe. Anything doctor must skip under a hook needs an explicit flag in the hook command (the harness-profiles plan adds --preflight). Codex trusts hooks by hash, so changing that hook command makes Codex users re-approve it.
