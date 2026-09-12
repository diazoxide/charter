# A chat opened in a workspace (the frame plus button, frame-new-chat, a w

_2026-09-10 14:40 · persistent_

A chat opened in a workspace (the frame plus button, frame-new-chat, a workspace tab) starts with its cwd set to workspaces/WS, and Claude Code reads project settings only from the session's own cwd without walking up. Charter mirrors only enabledPlugins and env into workspaces/WS/.claude/settings.json (claude_code.py WORKSPACE_KEYS), so NO permission rule from the plane root is in force there: charter guard ask rules and hand-written deny rules included. Any design that relies on an ask rule reaching the chat that runs a command must mirror the restrictive buckets (ask, deny) into workspace settings, never allow. Found 2026-09-10 while planning charter handoff.
