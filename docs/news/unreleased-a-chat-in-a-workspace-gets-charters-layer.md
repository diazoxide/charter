---
version: unreleased
headline: a chat launched in `workspaces/<ws>/` gets charter's layer there — the status line, the plugin and `$CHARTER_HARNESS` — and the two harnesses that cannot hold one say so instead of showing a tick
---

*"in workspace user should be 100% isolated."*

A chat whose cwd is `workspaces/<ws>/` — which is where the `+` and every workspace tab
put it — got **none of charter's settings**. Measured on this plane:

```
plane root      .claude/settings.json  { env, statusLine, enabledPlugins }
workspaces/*/   nothing — not one had a .claude
```

Claude Code reads project settings from the session's working directory and **does not walk
up**. Agents and skills *do* walk up and stop at a git boundary, and a workspace directory
is not one — it is a plain directory inside the plane's own repo — so those already arrived.
`CLAUDE.md` walks up too and, measured later in this release, is not git-bounded at all, so
it arrives from further still. What did not was everything that comes from a settings file: no plugin, no
status line, no `$CHARTER_HARNESS`. On this plane `fleet.1` and `opencode-integration.1`
had been running that way all along.

## What is written now

One generated file per workspace, `workspaces/<ws>/.claude/settings.json`, holding the
plane's own `enabledPlugins`, `statusLine` and `env` and nothing else. A 1:1 sync of the
three keys; `workspace.json` overrides come when somebody wants a specific one.

**All three, and the reason is that the plugin alone is not the layer.** Enabling
`charter@charter` there brings the hooks and the skills. `statusLine` has no plugin surface
at all, and `env` is where `$CHARTER_HARNESS` comes from on a harness with no per-shell
hook — so a workspace given only the plugin still renders no status line and still cannot
say which harness it is.

**Nothing else, deliberately.** No `skills/`, no `agents/`, no `CLAUDE.md`. Skills arrive
with the plugin and agents already walk up; a second copy of either would shadow the
plugin's own non-deterministically, and Claude Code says so in its own words — *"is already
taken by X, which takes precedence"*.

**Nothing at all inside a clone — and that limit did not survive the same release.** It
was stated here rather than discovered: `workspaces/<ws>/<repo>/` is a repo charter does
not own, `git add -A` there would stage whatever charter left behind, and so *in a clone
you cannot delegate to a persona*. #870 answers it in the same version, by paying the cost
this entry says below is not incurred — see *a clone gets the layer, and hides it in the
clone's `info/exclude`*.

## Written when, and owned how

Lazily, at launch — `charter workspace reinit` is the repair, and `--all` after this
upgrade. `charter doctor` grows a **`workspace layer`** row that regenerates the document
and compares, the same test `persona lint --only stale` makes of a generated sub-agent
rather than a second notion of staleness: what "current" means is whatever the generator
says today, and the only way to know that is to run it.

Ownership is a `.charter-generated` sidecar beside `.charter-structure`, holding a hash of
what charter last wrote — **not a key inside the vendor's JSON**, for the same reason
symlinking `.claude/` was wrong. `persona sync-agents` can put its marker inside the file
it generates because Markdown has a comment syntax; JSON has none. A file whose hash
matches is charter's and gets refreshed when the plane's settings move. A file whose hash
does not is the operator's: left completely untouched, never repaired, and named in the
`doctor` row.

## The workspace exists before a chat is put in it

`_launch_root` used to hand back the plane root when the workspace had no directory yet.
That was a real state rather than a defensive one — a plane draws a tab for its default
workspace whether or not it has been made — and it created a disagreement at the moment of
launch that nothing downstream could resolve: the chat recorded `workspace = <name>`
beside `cwd = <plane root>`. It now calls `workspace.ensure`, which is what `charter
--workspace <ws>` typed in the plane would have done, and it still degrades to the plane
root for a name that cannot be a workspace or a `workspaces/` charter cannot write.

## opencode and Codex are named, not skipped

A workspace **directory** is not a config scope for either. opencode reads
`~/.config/opencode/` plus an `opencode.json` at the repository **root**; Codex has no
project-level config **file** at all — a `.codex/config.toml` beside a project is ignored,
measured by planting a deliberate type error in one and watching the config load anyway. So
charter's layer is already live in a workspace on both, and there is nowhere for two
workspaces on one machine to differ.

(Both harnesses *do* read other things from a project — `.opencode/agent/` and
`.codex/skills/` — which a later measurement established and this entry originally denied.
Neither is a config scope, so the ceiling above is unchanged.)

That is a ceiling, and each now declares it as a `Deficit` — visible in `charter harness
list` and in the `workspace layer` row. Reporting one row for the harness that can and
nothing for the two that cannot would have read as three ticks.

## Why this is not what ADR 0015 deleted

ADR 0015 removed a per-tree design of exactly this shape, and the caution is real: *"255
lines and a test file"*. What it removed wrote the same **global** answer into every clone
and worktree for a harness that reads one global file anyway — *"all correct, and all
answering a question that does not need asking"*. This writes config that is **meant** to
differ per workspace, into a directory charter itself creates.

The staleness bookkeeping comes back, and is paid for by the `doctor` row above. The
`.git/info/exclude` entry per checkout does not arrive **for the workspace directory**:
`/workspaces/*/*` is already in the plane's `.gitignore` and the managed LIVE block
un-ignores five named paths, none of them `.claude/`. Nothing generated there can reach a
commit. One directory deeper it does arrive, and #870 is the entry that argues for it.

## To adopt

```
charter workspace reinit --all
```

The workspace structure version moved to 4, so every workspace made by an earlier charter
flags itself in the status line and in `charter workspace list` until this runs.
