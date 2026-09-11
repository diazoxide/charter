---
version: unreleased
headline: `doctor` answers for the Claude Code config folder you are actually using
---

Run Claude Code with `$CLAUDE_CONFIG_DIR` — a second account is the usual reason — and
`charter doctor` contradicted itself. `plugin install` said charter's plugin was not
installed for the plane, while `plane-root guard` stayed green with *wired (enabled plugin
charter@charter) — and it has fired here*. The green row was the wrong one: under that
folder no charter hook runs at all.

`plane-root guard`, `guard seen`, `session root` and `mcp` now read the folder Claude Code
reads: `$CLAUDE_CONFIG_DIR` when it is set, `~/.claude` and `~/.claude.json` when it is not.
`plugin install` and `plugin files` already did, by asking Claude Code. A guard sighting now
records the folder it ran under and counts for that folder only. Both guard rows stay yellow,
and say which case it is, for a sighting from another folder, one from before this release, or
one that names no harness. An empty or relative `$CLAUDE_CONFIG_DIR` is reported on both rows
whatever the sightings, with the only fix that changes them: an absolute path.

**What does not follow it.** The `personas` row still looks for a persona's skills under
`~/.claude`. `charter reinit` keeps deciding from `~/.claude` on purpose: it writes the plane's
committed `.claude/settings.json`, which every folder's sessions read. Three narrower Claude
Code settings are not followed either, so with any of them set these rows read the wrong file:

- `$CLAUDE_CODE_PLUGIN_CACHE_DIR` moves the installed-plugin list that `plane-root guard` and
  `guard seen` read.
- `$CLAUDE_CODE_USE_COWORK_PLUGINS` renames `plugins/` to `cowork_plugins/` and
  `settings.json` to `cowork_settings.json`.
- `$CLAUDE_CODE_CUSTOM_OAUTH_URL` renames the `.claude.json` that the `mcp` row reads to
  `.claude-custom-oauth.json`.

After upgrading, both guard rows say the last sighting predates folder recording. Run any Bash
command in a Claude Code session and the next `charter doctor` is green again.
