# A PreToolUse hook's `ask` decision FLOORS at a prompt — no harness permi

_2026-08-19 14:22 · persistent_

A PreToolUse hook's `ask` decision FLOORS at a prompt — no harness permission mode can lift it. Claude Code changelog: 'Fixed auto mode overriding a PreToolUse hook's ask decision for unsandboxed Bash — a hook ask now floors the decision at a prompt.' Consequences for charter: (1) 'delete the git hooks and let the harness handle autonomy' cannot work for an ask; (2) every charter _ask() is an unconditional stop even under bypassPermissions; (3) ask is the enemy of an unattended run, deny is harmless (a deny returns instantly and the agent routes around it). Also: permission_mode IS in the hook payload — Claude Code values default/auto/plan/bypassPermissions, and Codex REQUIRES the same field name. charter needs no autonomy env var or config; it reads what the harness declares.
