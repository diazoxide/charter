---
version: unreleased
headline: `doctor` answers for the Claude Code config folder you are actually using
---

Run Claude Code with `$CLAUDE_CONFIG_DIR` — a second account is the usual reason — and
`charter doctor` contradicted itself. `plugin install` said charter's plugin was not
installed for the plane, while `plane-root guard` stayed green with *wired (enabled plugin
charter@charter) — and it has fired here*. The green row was the wrong one: under that
folder no charter hook runs at all.

The rows about Claude Code's plugin, settings and MCP servers now read the folder Claude Code
reads. That is `$CLAUDE_CONFIG_DIR` when it is set, and `~/.claude` and `~/.claude.json` when
it is not. The rows are `plane-root guard`, `guard seen`, `session root` and `mcp`;
`plugin install` and `plugin files` already asked Claude Code. A guard sighting now records
the folder it ran under, and it vouches for that folder only. `charter reinit` asks the same
question before writing the guard hook, so run it from the shell you start Claude Code from.
The `personas` row does not follow the variable yet: it still looks for skills under
`~/.claude`.

One thing to expect after upgrading: `plane-root guard` may warn *nothing has fired here
under ~/.claude yet* on a plane whose plugin is fine. The sighting on disk is from before the
folder was recorded, so it cannot vouch for one. Run any Bash command in a Claude Code session
and the next `charter doctor` is green again.
