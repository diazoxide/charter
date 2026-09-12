# Measured 2026-09-12 on claude 2.1.269: 'enabled' in `claude plugin list

_2026-09-12 19:35 · persistent_

Measured 2026-09-12 on claude 2.1.269: 'enabled' in `claude plugin list --json` is the EFFECTIVE enabledPlugins value for the plugin id, merged local > project > user and resolved at the probe's cwd — every listed entry of that id carries the same value, and a disable written only to <cwd>/.claude/settings.local.json or only to <cwd>/.claude/settings.json flips it. The ENTRIES are install records, listed whatever the cwd. Ancestor settings asymmetry, reproduced: <ancestor>/.claude/settings.json does NOT reach a session in a subdirectory, while <ancestor>/.claude/settings.local.json does. So a project-scope install at a plane root only reaches a chat in workspaces/<ws>/ because charter mirrors enabledPlugins there (claude_code.WORKSPACE_KEYS).
