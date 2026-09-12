# Where Claude Code 2.1.267 reads project settings, measured with claude -

_2026-09-11 00:06 · persistent_

Where Claude Code 2.1.267 reads project settings, measured with claude -p in throwaway repos (PR 948, 2026-09-10): .claude/settings.local.json is read from the GIT ROOT and applies in any subdirectory session, and a subdirectory's own local file applies there too. .claude/settings.json is read only from the session's own cwd; the git root's copy does not apply in a subdirectory. A nested clone is its own git root and ignores the outer repo's local file; a linked worktree reads its MAIN checkout's local file. Charter's model before PR 948 treated settings.local.json as cwd-only (harness/claude_code.py LAYER, doctor._settings_files). Claude Code's docs date the git-root behaviour from v2.1.211. Consequence: a chat in workspaces/WS inside the plane repo already obeys the plane's own settings.local.json, but not its settings.json.
