---
name: persona
description: Work as a purlis persona, create one, or dispatch work to another — a role with its own charter, memory and vault. Use when asked to act as a role, to create a persona, or when deciding which persona a piece of work belongs to.
---

# Personas

A persona is *who does the work*. It is a role with a written charter, its own memory and its
own vault. A chat runs as one persona for its whole life: it takes on that persona's
responsibilities, its conventions and its credentials, and nobody else's.

The full mechanics are in `purlis docs show personas`: the charter format, inheritance, the
memory model and roster health.

## Know which one is active

```bash
purlis persona current        # the active persona's name, or (none)
purlis persona list           # every persona, the active one marked, each vault's state
purlis persona show <name>    # its charter: the role to actually adopt
purlis persona where          # in a chat purlis started: who asked for it, its sibling tasks,
                              # and where else this persona is working right now
```

The first rung that names a persona wins:

1. `--persona`
2. `$PURLIS_PERSONA` (a value that is empty or only whitespace counts as unset)
3. this session's selection
4. this terminal's selection (3 and 4 are both written by `purlis persona use`)
5. the plane-wide `.purlis/active-persona` (a shell with no session or pane id)
6. `purlis.toml` `[persona] default`
7. `personas/.default`
8. none

A selection can name a persona that no longer exists, for example one left behind by
`purlis persona remove`. It still wins, and it resolves to **no persona**, not to the
default. `purlis persona list` says so and names the way out. For a selection that is
`purlis persona use <name>` or `purlis persona clear`. For the variable it is unsetting
`$PURLIS_PERSONA`.

## Work as the one this chat runs as

1. Read its charter with `purlis persona show <name>` and behave as that role: its
   responsibilities, its focus, its definition of done.
2. Take credentials from its vault and nowhere else, and never print them. See the `secrets`
   skill.
3. Stay in role. **A chat's persona does not change.** In a chat purlis started, `purlis
   persona use` is refused and says the two ways forward: the operator allows the vault on
   this chat's tab, or you dispatch to the persona that owns the work.

## Give work to another persona: dispatch

Every persona declares a `delegate-when:`: the work that belongs to it. `purlis persona list`
shows who exists.

- **Clear match: dispatch** the work to that persona rather than doing it yourself. A dispatch
  starts a chat of its own that runs as that persona, with *its* vault, hosts and tools, and
  reports back to this chat:

  ```bash
  purlis dispatch --to <persona> --name "<a short name for the task>" <<'BRIEF'
  What you need done, what you already know, and what to report back.
  BRIEF
  ```

- **Partial or ambiguous match:** name the persona you would use and ask before dispatching.
- **No persona fits:** say so. Offer to create one only when the domain is large enough to
  own. A charter alone, with no credential or tool behind it, loses to a general-purpose
  helper.
- **Only dispatch to personas that exist.** Never invent a name.

**The first dispatch to another persona asks the person.** Nothing starts until they answer,
and you cannot answer for them. A persona's definition may say which personas it usually
works with:

```yaml
wants: [devops, qa]
```

`wants` grants nothing. It only adds an unticked box per persona under that question, so the
person can allow several pairs in one answer. Writing a name there, or `*`, does not let a
chat dispatch to anyone: the grants are the person's, kept where no chat writes them. Names
that are not finished personas of this project, the persona itself and `*` are ignored, and
`purlis persona lint` says so.

**A persona is never a sub-agent.** Do not start one with your harness's agent tool: a
sub-agent runs inside this chat, with this chat's vault and hosts, so it could not do that
persona's work anyway. A sub-agent call named for a persona is refused and names the
dispatch route. A helper that is not named for a persona (your harness's own, such as an
explorer) still runs, as this chat's persona, and is the right tool for work this chat's own
persona owns.

## Capability handoff

A persona reaches only its own vault and its own declared tools. When a task needs access it
does not have, **dispatch that step to the persona that holds it**. Do not guess with partial
credentials or borrow a secret. Knowing that another persona has a capability does not give
you access to it. Only an explicit `uses:` shares tools.

## Memory

A persona remembers across sessions. Search rather than reading everything:

```bash
purlis recall "<keywords>"                            # every base at once, labelled by source
purlis persona remember <name> "<durable fact>"       # persistent, committed
purlis persona remember <name> "<fact>" --shared      # for every persona
```

In a chat purlis started, the `persona_remember` tool does the same for the persona this chat
runs as (or, with `shared`, for every persona), and purlis writes it even where the chat's
sandbox would not let it.

Record what the work *taught* you, such as a decision, a gotcha or a verified fact. Do not
record what the repo already records. **Never put a secret in memory**; secrets belong in the
vault.

Memory is kept up the same way a workspace's is:

```bash
purlis persona dedupe <name>                  # near-duplicate pairs, to forget one of
purlis persona edit-memory <name> <slug> [--title "<title>"] ["<body>" | -]   # rewrite in place
purlis persona archive-memory <name> <slug>   # out of every list, into memory/archive/
purlis persona unarchive-memory <name> <slug> [--as <slug>]   # back from the archive
purlis persona move-memory <name> <slug> --to-shared   # or --to-persona, --to-workspace
purlis persona forget <name> <slug>           # delete one memory
purlis persona optimize                       # read-only curation report; --apply the safe ops
```

Each of `edit-memory`, `archive-memory`, `unarchive-memory` and `move-memory` takes `--shared`
for the `_shared` store. A move takes a memory whole to the scope it belongs to, its title and
date kept, and refuses a target that already holds one of that name. An edit keeps the memory's slug and its date, so anything that names it still
does; prefer it to forgetting a memory and writing it again. Archive is what the window's Delete
does, and it can be undone; `forget` cannot.

## Creating one

```bash
purlis persona create <name> --role "<Role>" --delegate-when "<the work that comes to it>" [--with-vault] [--use]
```

This writes a committed `personas/<name>/persona.md` as a **draft**. `--delegate-when` is
required unless `--extends <parent>` inherits it. No chat is dispatched to a draft. To finish
it:

1. Write what the persona owns and how it works.
2. Delete the `draft: true` line.
3. Commit it. Personas are shared.

A persona earns its place by carrying something a general-purpose agent cannot have: a
credential, a tool, or a domain narrow enough to name.

```bash
purlis persona lint               # dangling uses:/extends:, missing role/vault/delegate-when, keys nothing reads, wants: names not offered
purlis persona remove <name>      # refused while another persona extends or uses it
```

## Guardrails

- Work as one persona at a time, and never mix two personas' vaults.
- Editing or removing a persona changes a committed file. Commit it.
- A chat as a persona is started with that persona's MCP servers (`personas/<name>/mcp.json`)
  and its denied tools (`disallowed-tools:`) on Claude Code. If the briefing says a server
  was not started because nobody approved it, tell the operator: `purlis persona approve-mcp`
  is theirs to run in a terminal, and it is refused inside a chat.
- purlis generates no sub-agent for a persona, and `purlis persona sync-agents` is retired. A
  project that still has the files it wrote removes them with `purlis doctor --fix
  persona-agents`, which the operator runs: it changes committed files.
