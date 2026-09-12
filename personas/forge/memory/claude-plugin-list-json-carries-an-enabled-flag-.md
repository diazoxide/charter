# claude plugin list --json carries an enabled flag per install record, bu

_2026-09-10 17:53 · persistent_

claude plugin list --json carries an enabled flag per install record, but it is computed from the CURRENT DIRECTORY's resolved enabledPlugins, not from the record's projectPath: measured 2026-09-10 on Claude Code 2.1.267, every charter@charter record read true from a plane root and from a wired clone, false from a scratch dir and from a git worktree of that clone. It is a cheap read-only way to ask whether the host enables a plugin for a given directory without starting a session; unset CLAUDECODE and the CLAUDE_CODE_* session vars on that line when running it from inside a session. It is the host's flag, not proof a session's hooks fired.
