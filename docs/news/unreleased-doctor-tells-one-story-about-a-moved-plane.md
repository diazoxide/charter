---
version: unreleased
headline: `plane-root guard` names where a moved plane's plugin is installed, and `doctor`, the session preflight and `init` survive any install list
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
! plane-root guard  enabled plugin charter@charter is installed for <old path> and not for this directory, so no charter hook runs in a session here — branch moves in the plane root are NOT refused
```

Run `charter doctor --fix` from the plane. It installs the plugin for the new path, and the next
session there runs charter's hooks again, including sessions in the plane's workspaces.

**0.62.0 crashed on some install lists.** In these shapes, `plugins/installed_plugins.json` made
`charter doctor` print a traceback and no rows. The SessionStart preflight printed "charter
preflight failed" with that traceback at every session start, and `charter init` and `charter
reinit` stopped. The shapes are:

- a top-level array;
- `plugins` that is not an object;
- a record anywhere that is not an object;
- a version-1 list;
- records given as a number.

A chat can write that file.

Measured on 2.1.272, Claude Code checks the whole list before it loads anything, and a single
record it cannot read makes it load no plugin at all. Charter now reads the list only as Claude
Code 2.1.272's schema defines it: `version` 2, plugin ids of the form `plugin@marketplace`, and
every field that schema types holding the type it requires. For any other list, including a
version-1 list and one nested too deeply to parse:

- `plane-root guard` says it could not tell which directory the plugin is installed for, and names
  the file. If this session's settings declare the guard, the row stays green and says it could
  not tell whether a plugin also dispatches it.
- `charter init` and `reinit` write the guard hook as though no plugin dispatched it. A guard
  declared twice is harmless and `doctor` reports it; a guard declared nowhere is not.

A newer Claude Code that changes the schema gets the same answer until charter follows the change.

The same row also handles three more cases:

- **The install's files are gone.** Claude Code loads nothing from an install whose files are
  gone. The row says so and names `charter doctor --fix`: listing the plugins puts the files back,
  and a new session on its own does not.
- **Your settings declare the guard.** If this session's `.claude/settings.json` declares
  `charter hook pretooluse` itself, the row stays green whatever the install list says, because
  that block runs either way.
- **`$CLAUDE_CODE_PLUGIN_CACHE_DIR` is set.** Claude Code then reads its install list from that
  directory, which charter does not follow, so the row says it could not tell.

In this row, charter now escapes a newline or a terminal escape in a path or plugin id it takes
from the install list, and prints the rest as one line of plain text.

**`charter init` no longer writes into a settings file Claude Code refuses.** `JSON.parse`
refuses `NaN`, `Infinity` and `-Infinity`, so a `.claude/settings.json` holding one is a file
Claude Code loads nothing from. `init` read it with Python's own parser, which accepts them, and
wrote `env.CHARTER_HARNESS` and the `charter handoff` ask rule into it — and then printed that it
had left the file completely untouched. Both halves are fixed. Charter now reads that file, and
its machine-local sibling `.claude/settings.local.json`, exactly as Claude Code reads them, and a
settings file charter cannot read that way is never rewritten: `init` names it, writes nothing
into it, and exits 1.

The same now holds for a settings file too deeply nested for Python to write back — on 3.12 a
6,000-level file parses and then cannot be re-encoded. Every writer of a settings file refuses
it and leaves the file byte for byte as it was: the plane-root guard hook, the harness `env`
key, the `ask` and `allow` rules in both settings files, opencode's two `opencode.json` writers,
and Codex's `config.toml`. Before this, `charter init` and `charter guard` died with a traceback
on one — and `charter guard`, which writes every harness or none, could reach that traceback
after it had already written the earlier harness.

**`plane-root guard` reads a plugin's `hooks.json` as Claude Code runs it.** The row asked
whether the text `charter hook pretooluse` appeared anywhere in that file. That counts it in a
`matcher`, beside the command rather than in it, under another event entirely, in an entry Claude
Code would not run as a command — and in `charter hook pretooluse-read`, which is a different
handler guarding Read and Grep rather than Bash. So a plugin wiring only the read handler was
read as wiring the Bash guard, and `init` then wrote no hook for a plane nothing guarded. The row
now decides from the hook entry Claude Code would actually run.

A plugin's `hooks.json` that charter cannot parse is reported as what it is: the row says it
could not tell whether that plugin dispatches the guard, and names the file, rather than saying
the guard is not wired. `charter init` and `reinit` write the guard hook in that case anyway —
the safe direction, since a guard declared twice is harmless and reported while one declared
nowhere is a hole.

**The row and `charter init` now read your settings file the same way.** `plane-root guard`
decided whether the guard was declared by looking for the text `charter hook pretooluse`
anywhere in `.claude/settings.json`, while `init` decided structurally. So on three shapes they
disagreed about the same file: the guard's name in a `matcher`, in an entry whose `type` is not
`command`, and in `charter hook pretooluse-read` — a different handler, which guards Read and
Grep rather than Bash. The row printed a green `wired` over a plane where nothing runs the
guard, and `init`, reading the same file, went on to write the hook. Both now ask one reader,
so whatever the row calls wired is what `init` finds already present. And a settings file
charter cannot parse is reported as that — "could not tell whether the guard is declared here",
naming the file — rather than as a plane with no guard.

**And `guard seen` no longer asserts a deletion it did not see.** With that same file unreadable
and a plugin dispatching the guard, the row said the sighting came "from a settings declaration
that is no longer there" — sending you to look for an edit nobody had made, on the strength of a
file charter never read. It now says it could not tell whether that declaration is still there,
and names the file. The row still warns either way: nothing vouches for a declaration charter
cannot see.
