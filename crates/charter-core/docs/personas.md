# Personas

A **persona** is a role identity a chat adopts — `devops`, `qa`, `keycloak-master`, whatever
your work needs. Workspaces decide *which repos*; personas decide *who is working* and *what
they know*.

The CLI's `charter persona` has `create`, `show`, `list`, `use`, `current`, `clear`,
`default`, `remove`, `lint`, `remember`, `recall`, `secret`, `sync-agents` and `stats`.

```
charter persona create qa --role "QA Engineer" --delegate-when "test plans, flaky suites"
charter persona show qa                    # its metadata and the charter it adopts
charter persona list                       # who exists, who's active, each one's vault
charter persona use devops                 # the active persona for this session + this pane
charter persona clear                      # drop this session's, pane's and plane-wide choice
charter persona lint                       # dangling uses:/extends:, missing role/vault, stale agents
charter persona sync-agents                # a Claude Code sub-agent per persona, in .claude/agents/
charter persona stats                      # roster health: memory, verification, dispatches
charter persona remove qa                  # refused while another persona extends or uses it
```

`create` writes `personas/<name>/persona.md` as a **draft** (`draft: true`), with its
`memory/` and `refs/`. `--delegate-when` is required unless `--extends` names a parent to
inherit it from; a value holding a line break or `---` is refused, because each is written as
one frontmatter line. While the draft line is there no sub-agent is generated: write what the
persona owns, drop the line, then `charter persona sync-agents`. `--with-vault` registers its
vault as `charter vault add <vault> --persona <name>` would, and `--use` selects it.

`charter doctor` runs the same lint: its `personas` row summarises the roster, and `persona
grant` warns when the active persona is broken and its `tools:` are still approved.

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
| `vault` | The vault `charter persona secret` reads for this persona; `none` says it holds no credentials. Shown by `persona list` and named in its generated sub-agent ([secrets.md](secrets.md)). |
| `activity` | `orchestrator`, `standby` or `advisory`: memory volume is not a usage signal for this persona, so `persona stats` does not call it dormant. |
| `draft` | `true` while the charter is unfinished: `sync-agents` generates no sub-agent for it. |
| `agent-tools`, `disallowed-tools`, `skills`, `dispatch-isolation`, `model`, `color`, `memory`, `agent-description`, `description` | Read by `sync-agents` into the generated sub-agent — see *Sub-agents* below. |

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

`charter persona current` prints the name the ladder resolved, and `charter persona list`
prints it with the rung that decided, above the roster.

`charter persona use <name>` selects a persona: it writes the session pointer and, where
the terminal reports a pane id, the terminal pointer — so a pane keeps its persona across
closing and reopening the harness, and another pane is not touched. Only a process with
neither id writes the plane-wide `.charter/active-persona`. It says which of the three it
wrote, and warns when `$CHARTER_PERSONA` is set to something else, because that outranks
every pointer. Selecting a persona opens no vault.

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

## Sub-agents: `charter persona sync-agents`

`charter persona sync-agents` generates one Claude Code sub-agent per persona,
`.claude/agents/<name>.md`, from its resolved definition: the charter, a description the
router reads (built from `role`, `delegate-when`, `tools` and the vault unless
`agent-description` or `description` says otherwise), and the frontmatter the harness acts
on — `agent-tools` as `tools:`, `skills` (preloaded into the sub-agent at startup),
`dispatch-isolation: worktree` as `isolation: worktree`, `disallowed-tools` as
`disallowedTools:`, and `model`, `color` and `memory` as they are. `--persona <name>`
generates one.

Each file carries a ``GENERATED by `charter persona sync-agents` `` marker. A file at that
name without the marker is hand-written and is left alone; a full sync removes the generated
agents of personas that no longer exist. A persona marked `draft: true`, or whose frontmatter
spells a key charter reads in another case or declares one twice, gets no sub-agent, and a
generated one from before is removed — the generated file *is* the sub-agent's system prompt.

A persona's MCP servers live in `personas/<name>/mcp.json` (the `.mcp.json` schema) and are
declared in its sub-agent, so the harness starts them only when that persona is dispatched.
A server that declares `secrets` or `secret_files` is handed the persona's vault — wrapped in
`charter secret exec <vault> …` — only once this machine has approved the exact line
`sync-agents` prints for it:

```
charter persona sync-agents                # writes the agents; names every server it withheld
charter persona sync-agents --approve-mcp  # shows each one and asks, on a terminal
charter persona sync-agents --approve-mcp --dry-run   # shows them and records nothing
```

`--yes` approves every server without asking and is required off a terminal. The approval is
a digest of the line, kept in `.charter/mcp-approved.json`, so any change to the entry lapses
it. A server name outside letters, digits, `_`, `.` and `-` is refused and named.

Run from a linked worktree of the plane, `sync-agents` reads that worktree's `personas/` and
writes that worktree's `.claude/agents/`, and says so: the generated files belong to the
branch. The approvals and the vault registry stay the plane's.

The output is the Python charter's, byte for byte, so the first re-sync in a plane that
charter generated agents for before changes nothing that its personas have not.

## Roster health: `charter persona stats`

`charter persona stats [<name>]` reads the committed memory and the dispatch and skill logs:
per persona, how many memories it holds (`MEM`), how many are recent (`RECENT`,
`--recent-days`, 14 by default), the share carrying a verification word (`VERIFY`), the share
in a near-duplicate pair (`DUP`), and how often it was dispatched as a sub-agent (`DISP`). A
persona with no memory is `dormant`, one with none recent `idle`, a draft `draft`, and one
never dispatched while others were `never dispatched`. It also names skills a persona
declares and never uses, or uses and never declared, and how often routing advice fired
against the dispatches that followed it — advice only the Python charter gave, read from the
dispatch log it wrote; `routing:` is retired, and charter gives none now.

The dispatch tally is written by a hook as sub-agents return, and can miss background
dispatches, so `DISP` is a floor. Seeding it from past sessions' transcripts is not in this
version yet.

## Sub-agents in flight

When a chat dispatches a sub-agent (the `Task` or `Agent` tool) in a plane, charter records
it as in flight, and asks first when a persona that writes code is sent out while another
agent is still running in the same working tree.
