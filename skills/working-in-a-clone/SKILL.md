---
name: working-in-a-clone
description: Do real work inside a repo a workspace owns — build, test, change, commit. Use when asked to work on a named repo from a control plane, and to get the boundary right between the plane's session and the repo's own configuration.
---

# Working inside a clone

A control plane orchestrates repos; it is not where their work happens (ADR 0008). The
plane deliberately does not know how to build or test any of them — that knowledge lives
in each repo. This skill crosses that boundary correctly.

## 1. Know the workspace, and stay inside it

```bash
charter workspace current      # the active workspace, and how it resolved
```

Clones live at `workspaces/<workspace>/<repo>`. Work in the **active** workspace only.
A repo needed by this task that sits in another workspace gets cloned into this one —
never reached across.

## 2. Make sure it is cloned

```bash
charter clone <repo>           # skipped if already present; checks out its real default branch
```

Default branches differ across an org (`main`, `master`, `develop`). `charter clone` reads
each repo's actual default branch, so never assume one.

## 3. Adopt the repo's own conventions before editing

Read whichever of `CLAUDE.md`, `AGENTS.md`, `README.md` the repo ships, then skim its
manifest (`package.json`, `pom.xml`, `pyproject.toml`, `go.mod`) for its real build and
test commands. Use those — never commands carried over from a different repo.

## 4. Commit to the repo you are in

A clone is its own git repository, so committing there touches *its* history and never the
plane's. Push per that repo's workflow.

The plane's own tracked files are a separate concern — `charter save` commits and pushes
those.

## The boundary that is easy to get wrong

Claude Code binds configuration to the **project root fixed when the session launched** —
the plane. Two layers resolve differently:

- **Content loads natively.** A clone's `CLAUDE.md` / `AGENTS.md` is pulled into context as
  you work in its subtree. That is enough for cross-repo work and light edits, and the
  plane's own layer (personas, vaults, the tool gate, the status line) stays active.
- **The clone's `.claude/` stays inert.** Its `skills/`, `commands/`, `agents/`,
  `settings.json`, hooks, plugins and MCP servers belong to *that* project root. `cd` does
  not re-root the project, so they do not load here.

**Never import or run a clone's `.claude/` in the plane's session.** Those are executable
code belonging to another project, and running them from the plane merges two trust
boundaries that were separated on purpose.

To use them, open a session rooted in the repo — the supported way:

```bash
cd workspaces/<workspace>/<repo> && claude
```

There the repo's full configuration loads natively, and `charter` still works from inside
it when the control plane is needed.

Charter puts its own layer in the clone for that session — the plane's settings and its
`.claude/agents/`, which the clone's git root would otherwise cut off — and hides those
exact paths in the clone's `.git/info/exclude`. So a session rooted in the repo can
delegate to a persona, and the repo's `git status` is unaffected. `charter workspace
reinit` is the repair if a clone is missing it.

## Guardrails

- Confirm the working directory is `workspaces/<active>/<repo>` before a build or a commit.
- Never operate across two workspaces in one action.
- A clone you cannot access simply fails to clone — surface that rather than working around
  it. Partial access to an org is normal.
