# ALL THREE HARNESSES READ AN IN-REPO LAYER — measured 2026-09-04 against

_2026-09-04 00:10 · persistent_

ALL THREE HARNESSES READ AN IN-REPO LAYER — measured 2026-09-04 against codex-cli 0.147.0, opencode 1.18.23, Claude Code 2.1.259. Claude Code: .claude/settings.json is cwd-only with NO walk-up, while .claude/skills and .claude/agents walk up but stop at the git boundary. opencode: opencode.json at the repo root IS read (malformed JSON fails the run outright), and .opencode/agent/ IS read (opencode agent list shows the project agent). Codex: .codex/config.toml is NOT read from a project (malformed TOML causes no error, in a git repo or out), but .codex/skills/ IS read (a sentinel SKILL.md reaches the session context with zero tool calls). charter/harness/codex.py:56 says Codex has no project-level config; that is right about config.toml and wrong as a conclusion about Codex having no in-repo surface.
