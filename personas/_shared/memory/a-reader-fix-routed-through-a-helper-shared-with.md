# A reader fix routed through a helper shared with a writer changes the wr

_2026-09-11 13:07 · persistent_

A reader fix routed through a helper shared with a writer changes the writer. PR 970 (#969) made doctor's _plugin_declaring_guard follow CLAUDE_CONFIG_DIR; charter reinit shares that helper, so reinit began writing the guard hook into the committed .claude/settings.json based on one shell's config folder, doubling guard runs for everyone else. Before changing a helper's inputs, list its callers and pin every writer's decision with a test.
