---
version: unreleased
headline: On a plane that has moved, `plane-root guard` names the old path the plugin is installed for, and `charter doctor --fix`
---

Claude Code binds a project-scope plugin install to the directory it was installed from. Move
or rename a plane after `charter init` and that record keeps the old path. `claude plugin list
--json` still lists the install as enabled, but a session started at the new path loads no
plugin and runs none of charter's hooks. That was measured on Claude Code 2.1.272.

`plugin install` already warned that the plugin was not installed for the plane. `plane-root
guard` never checked which directory the install belonged to. It said the guard was wired for
the next session and told you to restart, and restarting changes nothing. If the guard had
fired before the move, the row was green: "wired (enabled plugin charter@charter) — and it has
fired here".

Now the guard row warns, names the path the install is recorded for, and gives the same fix as
`plugin install`:

```
! plane-root guard  enabled plugin charter@charter declares it, but Claude Code has it installed for <old path> and not for this directory, so no charter hook runs in a session here — branch moves in the plane root are NOT refused
```

Run `charter doctor --fix` from the plane. It installs the plugin for the new path, and the next
session there runs charter's hooks again, including sessions in the plane's workspaces.

The row reads that path from Claude Code's `plugins/installed_plugins.json`. If that file cannot
be read, is not the JSON Claude Code writes, or holds an install record charter cannot place,
the row warns that it could not tell which directory the plugin is installed for and names the
file. It points you at `claude plugin list --json`, which `charter doctor --fix` also reads. It
no longer answers as if it had read the file.

If the plane's own `.claude/settings.json` also declares `charter hook pretooluse`, the row no
longer calls that block a duplicate to delete. On a moved plane it is the only declaration a
session there loads, so the row stays green and says so.
