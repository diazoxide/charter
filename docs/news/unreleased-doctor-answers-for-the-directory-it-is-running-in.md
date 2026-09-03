---
version: unreleased
headline: '`charter doctor` reports the directory the session is rooted in, and says when that is not the plane'
---

*"I ran `charter doctor` in a chat that was clearly unguarded. It told me the plugin was
enabled and the guard was wired."*

## What was measured

A throwaway plane wired the way `charter init` leaves one — `enabledPlugins`, a
`statusLine`, and a `hooks.PreToolUse` entry running `charter hook pretooluse` in
`.claude/settings.json` — and a chat rooted at `workspaces/fleet/`, which is where the `+`
button and every workspace tab put one. That directory has no `.claude/` at all.

On the tree before this change:

```
cwd        : /…/plane/workspaces/fleet
config.ROOT: /…/plane
settings   : ['/…/plane/.claude/settings.json',
              '/…/plane/.claude/settings.local.json',
              '/…/home/.claude/settings.json']
guard row  : ok | wired (/…/plane/.claude/settings.json)
```

A tick, naming a file that session never reads.

## Why it read the wrong file

`doctor._settings_files` asked about `config.ROOT/.claude/settings.json`, and `config.ROOT`
comes from `root.find_root`, which **walks up** from the working directory until it finds a
`charter.toml`. That is exactly right for identity — the plane is who you are, from anywhere
inside it.

The host does not walk up. Claude Code reads project settings, agents, skills and commands
from the session's own directory and nowhere above it; only `CLAUDE.md` walks. So the two
answers differ for every chat that is not rooted at the plane, and doctor reported the one
the operator was not in.

**That is the shape of the defect, not the shape of one plane.** Doctor is what an operator
runs *because something felt wrong in that chat*, and a checker that reports on a different
directory than the one it is running in is wrong wherever it lands — a workspace, a clone, a
worktree, or anywhere you `cd`'d before launching.

## What happens now

A new `session root` row, and every row that reads settings answers for that directory:

```
  ✓  session root     /…/plane/workspaces/fleet — not the plane (/…/plane)
        ↳ the host reads project settings, agents, skills and commands from the session's
          own directory and does not walk up, so the plane's .claude/ is not in force here
          — the rows below read /…/plane/workspaces/fleet/.claude/ and ~/.claude/
        ↳ the plane is still this session's identity: personas, the vault, memory and
          workspaces resolve to /…/plane from anywhere inside it
  !  plane-root guard  pretooluse is not wired — branch moves in the plane root are NOT
                       refused, and nothing declares it in /…/plane/workspaces/fleet
```

The row is **OK, never a warning**. A chat rooted in a workspace is the designed workflow;
a row that warns on the normal case is one operators learn to skip. What is actually missing
warns on its own row, where it can name its own remedy — and this row is the sentence that
stops that warning reading as a bug in doctor.

Charter says which is which rather than silently preferring either, in the vocabulary it
already uses for the split (`docs/control-plane.md` → *What follows the plane, and what
follows the tree*): **the plane is identity, the directory you are standing in is
artifacts.**

## The `ask rules` row had it too

`_ask_rules` read `permissions.ask` from the plane's `settings.json` by its own path, so
#851's fix did not reach it. `permissions` is a host settings key like any other and is
scoped to the directory the host read it from, so for a chat rooted at `workspaces/<ws>/`
the row was wrong in both directions: it reported the plane's `ask` rules as shadowing a
persona's declared tools when they never reach that session, and it could not see a rule in
the session's own settings that genuinely does. That row exists to name *why* pre-approved
tools started prompting (ADR 0014), so a wrong answer sends the reader hunting in a file the
host never opened. It now reads the same root every other settings row does.

## The remedy had the same defect one level down

`charter reinit` writes the **plane's** `.claude/settings.json`. Offered to a session rooted
elsewhere, that hint would have been followed, believed, and left the session exactly as
unguarded. So the guard row now names a file this session actually reads — the session
root's own `.claude/settings.json`, or `~/.claude/settings.json`, which every session reads
whatever it is rooted in.

## What this does not check, and what a green row does not mean

**Trust.** Claude Code gates hook execution and the status line on the directory being
trusted, globally, whatever declared them — so *"the plugin is enabled here"* and *"charter's
hooks actually run here"* stay two different questions, and an untrusted directory answers
yes to the first and no to the second. Reading `~/.claude.json` for a `hasTrustDialogAccepted`
flag would be one more proxy of exactly the kind this fix removes, and charter has not
measured that file's semantics — a directory with no entry has not been *refused*, it has
merely never been opened, and a checker that read absence as refusal would warn on planes
that are fine.

So, plainly: **a green `plane-root guard` row means "a file this session reads declares the
guard". It does not mean the guard will fire.** Those are two rows on purpose — `guard seen`
is the one that answers firing, and it answers from evidence that a guard actually ran. What
neither row asks is whether this directory is trusted, and an untrusted directory is one
where a declaration reads as present and nothing dispatches. `workspaces/<ws>/` inherits the
plane's trust because it is inside the plane's git repo; a clone or a linked worktree has a
git root of its own and needs its own acceptance, so that is where the gap bites. Filed
separately rather than guessed at here.
