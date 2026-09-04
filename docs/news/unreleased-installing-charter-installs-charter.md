---
version: unreleased
headline: Installing charter installs charter — `uv tool install charter-cp`, and `charter init` brings Claude Code's plugin with it
adopt: doctor --fix
---

The README used to ask for three commands and hope you ran all of them:

```bash
uv tool install charter-cp
claude plugin marketplace add diazoxide/charter
claude plugin install charter@charter
```

It now asks for one. `uv tool install charter-cp` puts the `charter` CLI on your `PATH`,
and `charter init` installs Claude Code's charter plugin for the plane it just created.
For a plane that already exists, `charter doctor --fix`.

## The CLI is the front door

The CLI is what sits on `PATH`, what all twelve hooks in `hooks/hooks.json` shell out to,
and what already writes `.claude/settings.json` — so it exists before a harness session
does. The inverse shape, a plugin that bootstraps a CLI, would have to guess at a Python
environment it does not own.

A third install of one of them was the failure this closes: the plugin ships **no Python**,
so a CLI-only install leaves every guard and every injection dead while looking completely
installed — and the person who skipped the second and third commands had no way to know.

## `charter doctor` gained a row and a flag

```
  !  plugin install   charter's Claude Code plugin is not installed for this plane, so none
                      of its hooks run here — no session context, no plane-root guard, no
                      auto-save
        → Run: charter doctor --fix  (installs `charter@charter` at project scope from
          `diazoxide/charter`; `charter init` does the same thing on a fresh plane). The
          plugin loads at the NEXT session, so restart afterwards.
```

**WARN, not FAIL**, and the distinction is load-bearing. `cmd_doctor` exits non-zero only
on FAIL, and that exit code is what makes the SessionStart preflight print — so a FAIL here
would put a red preflight in front of every CLI-only install, which `docs/install.md`
supports and CI uses. It would also be wrong about protection: a plane that declares
`charter hook pretooluse` in its own `.claude/settings.json` is guarded with no plugin at
all, and `plane-root guard` is the row that answers whether the guard fires. This row
answers whether the artifact charter can install for you is there.

The row is scoped to the **plane**, not the machine: a project-scoped install belonging to
somebody else's checkout is not an answer here, and reading one as an answer would print
"installed" over a plane with no plugin — #168's defect wearing a new row.

## Project scope, and never `user`

The plugin is what carries a plane's pinned version, which is why `[charter].version` is
measured against the plugin and not the binary: two planes on one laptop can sit on
different charters without fighting. A machine-wide install would collapse that to one
version per machine, and would put charter's hooks into repositories nobody pointed charter
at.

## Nothing self-heals

`charter init` and `charter doctor --fix` install. Nothing else does — not `charter
reinit`, which re-runs the *wiring*, and certainly not `charter workspace list`. A tool
that installs software as a side effect of answering a question is the surprise #857 is
about, and the same reason charter refuses to write `~/.claude/settings.json` unasked.

## Version skew: `doctor` refuses, hooks only warn

Unchanged, and now written down where it can be read. The two artifacts update through
different channels (`uv tool upgrade`, `claude plugin update`), so skew is normal rather
than exceptional and happens mid-session. A hook that hard-failed on it would refuse every
tool call in that session, turning a cosmetic mismatch into an outage — so the hooks warn
once at session start and keep guarding, and `charter doctor` is the surface that refuses.

## Only Claude Code

opencode's plugin already arrives with `charter init` — one file under
`~/.config/opencode/`, because that is the only place opencode reads for every project.
Codex's wiring lives only in `~/.codex/config.toml`, in force for every repository on the
machine, so charter writes it only when you run `charter harness install codex`, where
running the command is the consent. `charter doctor` reports that gap and stops there: one
documented one-time edit is a smaller cost than charter silently rewriting a machine-wide
config.
