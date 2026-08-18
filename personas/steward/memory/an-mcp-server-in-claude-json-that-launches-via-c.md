# An MCP server in ~/.claude.json that launches via 'charter secret exec <

_2026-08-17 13:21 · persistent_

An MCP server in ~/.claude.json that launches via 'charter secret exec <vault>' MUST also set CHARTER_ROOT in its env. A top-level mcpServers entry is USER-scope: it launches for every project, and a bare 'charter' resolves its plane from the launching directory, so the vault is only found when the open project happens to be the right plane. Symptom is the worst kind: the server starts, fails to find the vault, and the tools are silently absent. Verified 2026-08-17 on the elasticsearch server whose vault (elastic-logs-master) lives in the easydmarc-umbrella plane, not IdeaProjects/charter. charter doctor's hint says this from 0.38.1 onward.
