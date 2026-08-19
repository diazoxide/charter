# Editing personas/release/persona.md in a workspace clone: regenerate .cl

_2026-08-19 12:19 · persistent_

Editing personas/release/persona.md in a workspace clone: regenerate .claude/agents/release.md with CHARTER_ROOT=$PWD charter persona sync-agents. Without the override, find_root walks up past the clone's own charter.toml to the parent plane and rewrites THAT plane's agents instead of the clone's.
