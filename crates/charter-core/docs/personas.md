# Personas

A **persona** is a role identity a chat adopts — `devops`, `qa`, `keycloak-master`, whatever
your work needs. Workspaces decide *which repos*; personas decide *who is working* and *what
they know*.

In this version the CLI's `charter persona` has `current`, `default`, `remember` and
`recall`. Creating, listing, showing, linting, removing and switching personas from the CLI,
and generating a persona's sub-agent, are not in this version yet: a persona is added by
writing its directory, and a chat is started on one from the app's picker.

A persona lives in a **committed** directory, `personas/<name>/`:

```
personas/devops/
├── persona.md     # frontmatter (role, tools, …) + the charter itself (prose)
├── memory/        # persistent, committed knowledge — MEMORY.md index + one file per fact
├── refs/          # curated docs, links and snippets for the role, also committed
└── bin/           # optional: executables this persona carries
```

`personas/_shared/` holds the memory and refs every persona reads.

## The charter format

```markdown
---
role: DevOps Engineer
delegate-when: CI/CD pipelines, Kubernetes/GitOps, deploys, service infrastructure
tools: kubectl, glab
extends: platform-base
uses: qa
---

# DevOps Engineer

You are the **devops** persona — DevOps Engineer. …
```

The frontmatter is flat `key: value` lines. The keys this version acts on:

| Key | What charter does with it |
| --- | --- |
| `role` | Shown in the app's persona view, and quoted to a chat started as this persona. |
| `delegate-when` | What work belongs here. Shown in the persona view and quoted in the session briefing. |
| `tools` | **Programs** a chat on this persona runs without a permission prompt — see *The tool gate* below. |
| `extends` | Inherit another persona's frontmatter (see *Inheritance*). |
| `uses` | Other personas whose `tools:` this one may also run without a prompt, unless `borrows:` narrows it. |
| `borrows` | Which of those personas' tools are unioned in: a list of names, or `none`. |
| `vault` | Shown in the persona view. The vault commands are not in this version yet ([secrets.md](secrets.md)). |

Other keys are kept in the file and not acted on in this version.

Everything below the second `---` is the **charter** itself: free prose describing the role.

### Inheritance (`extends`)

The chain is read from the root ancestor down, so a child's value for a plain key replaces
its parent's, and an empty value does not override. `tools`, `agent-tools` and `uses`
accumulate along the chain, without repeats.

`borrows:` fails closed: if `borrows` or `extends` is misspelled or written twice anywhere in
the chain, the persona borrows nothing rather than every `uses:` persona's tools.

## Which persona a chat is on

A chat started from the app's picker on a persona gets `$CHARTER_PERSONA` set to that name,
and the picker refuses a name this plane has no persona for. Every `charter` command and hook
in that chat resolves it from there.

Outside that, seven rungs decide, highest first. The first one that names a persona wins:

| Rung | Where |
| --- | --- |
| `--persona <name>` | the flag, one command |
| `$CHARTER_PERSONA` | the environment |
| session pointer | `.charter/sessions/<id>.persona` |
| terminal pointer | `.charter/terminals/<id>.persona` |
| plane-wide file | `.charter/active-persona` |
| declared default | `charter.toml` `[persona] default` |
| legacy default | `personas/.default` |

`$CHARTER_PERSONA` is stripped of surrounding whitespace, and a value that is empty or only
whitespace counts as unset. A `--persona` that is only whitespace is refused.

**The two committed rungs name a persona only if it exists.** A rung above them that names a
persona that does not exist still wins, and the session has **no** persona — the plane's
default does not stand in for it, because that would hand the chat a persona, with its
tools, that nobody chose. The session briefing says so when it happens.

`charter persona current` prints the name the ladder resolved.

### The front door

A plane declares its default persona in `charter.toml`:

```toml
[persona]
default = "steward"
```

Set it with `charter persona default <name>`, or edit the file. `charter persona default
--clear` removes it from `charter.toml` and removes the legacy `personas/.default` too,
because either one left behind would go on answering. `charter init` creates no personas, so
a fresh plane has no front door until you declare one.

`personas/.default` is the older committed declaration. It still resolves, one rung below
`charter.toml`; prefer the TOML key.

Every command that takes a persona name checks it first, the same way:

```
✗ no persona 'devosp' …
✗ invalid persona name '../x' (lowercase letters, digits, '.', '_', '-')
✗ no persona ' ' (a persona name is never only whitespace)
```

## What a chat is told

When a chat starts in a plane, `charter hook sessionstart` briefs it: the persona it is on,
its `role:` and `delegate-when:` quoted as a description rather than an instruction, and the
newest titles from its memory. It is followed by the workspace, its open todos and the other
workspaces on the plane — see [hooks.md](hooks.md).

## Memory: a 2×2

Every persona's memory has two axes — **own or shared**, **persistent or ephemeral**:

|  | Persistent (committed) | Ephemeral (session scratch) |
| --- | --- | --- |
| **Own** | `personas/<name>/memory/` | `.charter/persona-state/ephemeral/<session>/<name>/` |
| **Shared** | `personas/_shared/memory/` | `.charter/persona-state/ephemeral/<session>/_shared/` |

The writer picks the quadrant:

```
charter persona remember devops "prod kubeconfig lives in the devops vault, key KUBECONFIG"
charter persona remember devops "the migration runbook is at ..." --shared
charter persona remember devops "trying approach X for this task" --ephemeral
```

Persistent memory is written into the committed tree. Committing it as it is written is not
in this version yet: when the plane's `[memory].share` is anything but `local`, `remember`
says so, and you commit and push `memory/` with git (or `charter save`). Ephemeral memory is
gitignored scratch for one session.

Read it back with `charter persona recall devops [--query "kubeconfig"]`, or search the
persona's own memory, the shared namespace, its refs and the active workspace's journal at
once with `charter recall "<keywords>"`.

Memory and refs are committed and shared with the team, so a credential written into one is
disclosed. A chat is told at once when a memory or ref it wrote looks like it holds one, and
`charter save` refuses to commit it — see [secrets.md](secrets.md).

## The tool gate

A persona's `tools:` lets a chat on that persona run those programs **without a permission
prompt**. The unit is the program, and every argument rides along: `tools: gh` is `gh` doing
anything. The gate only ever smooths — the worst it can do is decline, and a declined command
meets the harness's ordinary prompt. It declines:

- a command holding any character the shell would rewrite — `$`, `~`, `*`, `{`, `;`, `|`,
  `>`, quoted or not;
- interpreters and wrappers — `bash`, `python`, `env`, `sudo`, `xargs`, `find`, `make` — which
  run whatever their arguments say;
- a command with an argument that is itself an executable file;
- destructive subcommands — `kubectl delete`, `git clean`, `charter secret` and the like;
- anything that touches charter's own state, the vault directory or a persona definition, by
  any spelling, link or case-folded name;
- a command whose program is not the file the declared name refers to: a bare name only when
  the persona ships no script of that name in `bin/`, and a path only when it is that very
  file.

**The ceiling is frozen at session start.** `tools:` is a line in a file the chat itself can
edit, so `SessionStart` snapshots every persona's tools and the gate grants only what is in
both the snapshot and the live file. An edit can narrow a grant mid-session, never widen one.

Scripts in `bin/` are not put on `PATH`; call them by path, and declare their names in
`tools:` like any other program. `bin/` is committed, so on a shared plane it reaches
teammates' machines.

## Sub-agents in flight

When a chat dispatches a sub-agent (the `Task` or `Agent` tool) in a plane, charter records
it as in flight, and asks first when a persona that writes code is sent out while another
agent is still running in the same working tree.
