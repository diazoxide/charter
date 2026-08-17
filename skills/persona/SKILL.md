---
name: persona
description: Adopt, create, or route work to a charter persona — a role with its own charter, memory and vault. Use when asked to act as a role, to switch or create a persona, or when deciding which persona a piece of work belongs to.
---

# Personas

A persona is *who does the work* — a role with a written charter, its own memory, and its
own vault. Adopting one means taking on its responsibilities, its conventions, and its
credentials.

Mechanics in full — the charter format, inheritance, the memory model, roster health:
`charter docs show personas`.

## Know which one is active

```bash
charter persona current        # the active persona, and how it resolved
charter persona list           # all of them; * marks active; shows vault state
charter persona show <name>    # its charter — the role to actually adopt
```

Resolution: `--persona` → `$CHARTER_PERSONA` (pinned at launch) → local selection
(`charter persona use`) → the committed default (`personas/.default`) → none.

## Adopt one

1. Read its charter with `charter persona show <name>` and behave as that role — its
   responsibilities, its focus, its definition of done.
2. Take credentials from its vault, never from anywhere else, and never print them
   (see the `secrets` skill).
3. Stay in role until asked to switch.

**Do not switch the active persona unless asked.** Switching changes the session's identity
for everyone downstream; routing a task does not.

## Route work instead of switching

Every persona declares a `delegate-when:`, surfaced in its sub-agent description.

- **Clear match → delegate** to that persona's sub-agent rather than doing it yourself.
  Delegation isolates context: only the result comes back, and the persona runs with *its*
  vault and tools.
- **Partial or ambiguous → name the persona you would use and ask** before dispatching.
- **No persona fits → say so.** Offer to create one only when the domain is genuinely large
  enough to own; a charter alone, with no credential or tool behind it, loses to a
  general-purpose agent.
- **Only route to personas that exist.** Never invent a name.

A persona sub-agent is generated from its charter:

```bash
charter persona sync-agents        # regenerate after editing any charter
```

## Capability handoff

A persona reaches only its own vault and its own declared tools. When a task needs access
it does not have, **delegate that step to the persona that holds it** rather than guessing
with partial credentials or borrowing a secret. Awareness of another persona's capability
is not access to it — only an explicit `uses:` shares a vault.

## Memory

A persona remembers across sessions. Search rather than bulk-reading:

```bash
charter recall "<keywords>"                            # every base at once, labelled by source
charter persona remember <name> "<durable fact>"       # persistent, committed
charter persona remember <name> "<fact>" --shared      # for every persona
```

Record what the work *taught* you — a decision, a gotcha, a verified fact — not what the
repo already records. **Never put a secret in memory**; that is what the vault is for.

## Creating one

```bash
charter persona create <name> --role "<Role>" [--with-vault] [--use]
```

This writes a committed `personas/<name>/`. Edit the charter, then commit it — personas are
shared. A persona earns its place by carrying a capability a general-purpose agent cannot
have: a credential, a tool, or a domain narrow enough to name.

## Guardrails

- One persona at a time; never mix two personas' vaults.
- Editing or removing a persona changes a committed file — commit it.
- Re-run `charter persona sync-agents` after any charter edit, or the dispatchable
  sub-agent silently describes the old role.
