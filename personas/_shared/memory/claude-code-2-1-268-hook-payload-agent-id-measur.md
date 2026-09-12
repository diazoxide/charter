# Claude Code 2.1.268 hook payload agent_id, measured 2026-09-11 against a

_2026-09-11 12:27 · persistent_

Claude Code 2.1.268 hook payload agent_id, measured 2026-09-11 against a local mock Messages API (non-bare claude -p, bypassPermissions, hooks from --settings, throwaway HOME and CLAUDE_CONFIG_DIR). A main-conversation Bash PreToolUse payload carries NO agent_id, nor does the payload for the Agent tool call itself; a sub-agent's Bash payload carries agent_id and agent_type 'general-purpose'. --bare cannot measure this: it exposes only Bash (no Agent even with --tools) and skips --settings hooks. Keychain isolation for a non-bare probe: the binary reads credentials by running the shell string 'security find-generic-password ... -s <service>', so a fake 'security' first on PATH intercepts it; with CLAUDE_CONFIG_DIR set the service names were 'Claude Code-<hash>' and 'Claude Code-credentials-<hash>'.
