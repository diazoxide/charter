---
version: unreleased
headline: A `charter guard ask` rule is in force in the chat that runs the command
security: true
adopt: workspace reinit --all
---

`charter guard ask 'terraform apply *'` wrote the plane's `.claude/settings.json` and said
the rule *"applies to everyone on this repo"*. It did not apply in the chats where the
command actually gets typed. A framed chat stands in `workspaces/<name>/`, Claude Code reads
the shared project settings from the session's own directory and does not walk up, and the
file charter generates there carried `enabledPlugins` and `env` and never `permissions`. A
hand-written `permissions.deny` was dropped the same way, and every `doctor` row about that
chat was green.

The restrictive half now travels: `permissions.ask` and `permissions.deny` are mirrored into
the `.claude/settings.json` charter generates for each workspace and for each clone inside
one. **`permissions.allow` still never travels** — a grant copied sideways puts a permission
in force where nobody clicked for it, while a restriction only ever adds a prompt or a
refusal.

- A `--local` rule reaches a workspace without a copy. Measured on Claude Code 2.1.267:
  `.claude/settings.local.json` is read at the git root, and a workspace directory is inside
  the plane's repository. A clone is a git root of its own, so it gets a generated
  `.claude/settings.local.json` with the plane's local `ask` and `deny` — which is what
  `charter doctor`'s own recommendation, `charter guard ask --local 'charter change land *'`,
  needed to reach a chat in a clone at all.
- In a clone that file is also where Claude Code saves "Yes, and don't ask again". Charter
  hides it in the clone's `.git/info/exclude` before writing it, writes nothing it cannot
  hide, and keeps it hidden and untouched once Claude Code has added its approvals, so a
  machine-local rule never becomes committable in somebody else's repository.
- A file charter wrote in a clone now stays hidden for as long as it is there, even after you
  rewrite it: charter never overwrites it, and `charter doctor` names it and shows the
  `git add -f` that commits it if it is yours. This reverses 0.56.0, where a file you rewrote
  stopped being hidden at the next launch.
- `charter guard ask` refreshes every workspace as it writes, so the rule is in force when
  the command returns rather than at the next launch. It runs both ways: drop a rule from
  the plane and its mirror is withdrawn, while the generated file still matches what charter
  wrote. A plane settings file that does not parse keeps every workspace's last good copy.
- `charter doctor`'s `workspace layer` row names the consequence — *"a chat in that
  directory may not be prompted or refused by them"* — instead of only naming a stale file,
  and names a plane settings file it cannot read.
- `charter doctor` now counts a hook or a plugin declared in the plane's
  `.claude/settings.local.json` for a workspace chat, where Claude Code runs it.

Two limits stand, and both are named rather than papered over. Codex has no command-pattern
permissions, so there is nothing to carry there. opencode resolves `opencode.json` at the
repository root, so a workspace *directory* already reads the plane's copy — but a clone is a
repository root of its own and charter generates no `opencode.json` in it, so an opencode
session rooted inside a clone still does not have the plane's rules.

Existing workspaces pick the rules up on their next launch. `charter workspace reinit --all`
does it now.
