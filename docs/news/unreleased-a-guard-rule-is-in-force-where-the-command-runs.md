---
version: unreleased
headline: A `charter guard ask` rule is in force in the chat that runs the command
security: true
adopt: workspace reinit --all
---

`charter guard ask 'terraform apply *'` wrote the plane's `.claude/settings.json` and said
the rule *"applies to everyone on this repo"*. It did not apply in the chats where the
command actually gets typed. A framed chat stands in `workspaces/<name>/`, Claude Code reads
project settings from the session's own directory and does not walk up, and the file charter
generates there carried `enabledPlugins` and `env` and never `permissions`. A hand-written
`permissions.deny` was dropped the same way, and every `doctor` row about that chat was
green.

The restrictive half now travels: `permissions.ask` and `permissions.deny` are mirrored into
the file charter generates for each workspace and for each clone inside one. **`permissions.
allow` still never travels** — a grant copied sideways puts a permission in force where
nobody clicked for it, while a restriction only ever adds a prompt or a refusal.

- A `--local` rule stays local. `charter guard ask --local` writes the plane's gitignored
  `.claude/settings.local.json`, and it is mirrored into a generated
  `.claude/settings.local.json` of its own rather than folded into the committed sibling.
  That is what `charter doctor`'s own recommendation —
  `charter guard ask --local 'charter change land *'` — needed to reach a workspace chat at
  all.
- `charter guard ask` refreshes every workspace as it writes, so the rule is in force when
  the command returns rather than at the next launch. It runs both ways: drop a rule from
  the plane and its mirror is withdrawn, while the generated file still matches what charter
  wrote.
- `charter doctor`'s `workspace layer` row names the consequence — *"a chat in that
  directory is not prompted or refused by them"* — instead of only naming a stale file.

Two limits stand, and both are named rather than papered over. Codex has no command-pattern
permissions, so there is nothing to carry there. opencode resolves `opencode.json` at the
repository root, so a workspace *directory* already reads the plane's copy — but a clone is a
repository root of its own and charter generates no `opencode.json` in it, so an opencode
session rooted inside a clone still does not have the plane's rules.

Existing workspaces pick the rules up on their next launch. `charter workspace reinit --all`
does it now.
