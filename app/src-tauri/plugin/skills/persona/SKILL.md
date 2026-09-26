---
name: persona
description: Adopt, create, or route work to a charter persona — a role with its own charter, memory and vault. Use when asked to act as a role, to switch or create a persona, or when deciding which persona a piece of work belongs to.
---

# Personas

A persona is *who does the work*. It is a role with a written charter, its own memory and its
own vault. Adopting one means taking on its responsibilities, its conventions and its
credentials.

The full mechanics are in `charter docs show personas`: the charter format, inheritance, the
memory model and roster health.

## Know which one is active

```bash
charter persona current        # the active persona's name, or (none)
charter persona list           # every persona, the active one marked, each vault's state
charter persona show <name>    # its charter: the role to actually adopt
```

The first rung that names a persona wins:

1. `--persona`
2. `$CHARTER_PERSONA` (a value that is empty or only whitespace counts as unset)
3. this session's selection
4. this terminal's selection (3 and 4 are both written by `charter persona use`)
5. the plane-wide `.charter/active-persona` (a shell with no session or pane id)
6. `charter.toml` `[persona] default`
7. `personas/.default`
8. none

A selection can name a persona that no longer exists, for example one left behind by
`charter persona remove`. It still wins, and it resolves to **no persona**, not to the
default. `charter persona list` says so and names the way out. For a selection that is
`charter persona use <name>` or `charter persona clear`. For the variable it is unsetting
`$CHARTER_PERSONA`.

## Adopt one

1. Read its charter with `charter persona show <name>` and behave as that role: its
   responsibilities, its focus, its definition of done.
2. Take credentials from its vault and nowhere else, and never print them. See the `secrets`
   skill.
3. Stay in role until asked to switch.

**Do not switch the active persona unless asked.** Switching changes the session's identity
for everything downstream. Routing a task does not.

## Route work instead of switching

Every persona declares a `delegate-when:`, which appears in its sub-agent's description.

- **Clear match: delegate** to that persona's sub-agent rather than doing the work yourself.
  Delegation isolates context: only the result comes back, and the persona runs with *its*
  vault and tools.
- **Partial or ambiguous match:** name the persona you would use and ask before dispatching.
- **No persona fits:** say so. Offer to create one only when the domain is large enough to
  own. A charter alone, with no credential or tool behind it, loses to a general-purpose
  agent.
- **Only route to personas that exist.** Never invent a name.

Each persona's sub-agent is generated from its charter:

```bash
charter persona sync-agents        # regenerate after editing any charter
```

## Capability handoff

A persona reaches only its own vault and its own declared tools. When a task needs access it
does not have, **delegate that step to the persona that holds it**. Do not guess with partial
credentials or borrow a secret. Knowing that another persona has a capability does not give
you access to it. Only an explicit `uses:` shares tools.

## Memory

A persona remembers across sessions. Search rather than reading everything:

```bash
charter recall "<keywords>"                            # every base at once, labelled by source
charter persona remember <name> "<durable fact>"       # persistent, committed
charter persona remember <name> "<fact>" --shared      # for every persona
```

Record what the work *taught* you, such as a decision, a gotcha or a verified fact. Do not
record what the repo already records. **Never put a secret in memory**; secrets belong in the
vault.

Memory is kept up the same way a workspace's is:

```bash
charter persona dedupe <name>                  # near-duplicate pairs, to forget one of
charter persona forget <name> <slug>           # delete one memory
charter persona optimize                       # read-only curation report; --apply the safe ops
```

## Creating one

```bash
charter persona create <name> --role "<Role>" --delegate-when "<the work that comes to it>" [--with-vault] [--use]
```

This writes a committed `personas/<name>/persona.md` as a **draft**. `--delegate-when` is
required unless `--extends <parent>` inherits it. A draft gets no sub-agent. To finish it:

1. Write what the persona owns and how it works.
2. Delete the `draft: true` line.
3. Run `charter persona sync-agents`.
4. Commit it. Personas are shared.

A persona earns its place by carrying something a general-purpose agent cannot have: a
credential, a tool, or a domain narrow enough to name.

```bash
charter persona lint               # dangling uses:/extends:, missing role/vault/delegate-when, stale agents
charter persona remove <name>      # refused while another persona extends or uses it
```

## Guardrails

- Work as one persona at a time, and never mix two personas' vaults.
- Editing or removing a persona changes a committed file. Commit it.
- Re-run `charter persona sync-agents` after any charter edit. Otherwise the dispatchable
  sub-agent silently describes the old role.
