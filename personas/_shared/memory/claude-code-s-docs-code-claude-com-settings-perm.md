# Claude Code's docs (code.claude.com settings, permissions and worktrees

_2026-09-10 17:55 · persistent_

Claude Code's docs (code.claude.com settings, permissions and worktrees pages, fetched 2026-09-10) contradict two rules charter/harness/claude_code.py records. (1) .claude/settings.local.json is NOT cwd-only: it loads from the git repository root, and in a git worktree from the MAIN checkout's root (v2.1.211+); only .claude/settings.json keys (enabledPlugins, env, hooks, permissions) stay cwd-only with no parent fallback, yet _PROJECT_SETTINGS lumps both files as cwd-read. (2) Workspace trust in a worktree uses the main checkout's root, while trust_gate's comment and doctor's session layer trust line say a linked worktree needs its own acceptance. Docs, not re-measured; noted in #951.
