---
name: compact
description: Compact and improve a charter workspace's or persona's memory — find duplicates and stale entries with charter's optimize and dedupe reports, prune only with the operator's yes, and fold durable lessons into workspace.md or persona.md. Use when asked to compact, tidy, prune, dedupe or improve a workspace's or persona's memory.
---

# Compact & improve

Memory grows by one file per lesson, and nothing merges it. Compacting makes it smaller and
truer; improving moves what has become a rule out of the journal and into the charter every
chat reads first (`workspace.md` for a workspace, `persona.md` for a persona).

## 1. Read the reports

A workspace:

```bash
charter workspace optimize <name>        # exact duplicates, near duplicates, stale, index drift
```

A persona:

```bash
charter persona optimize <name>
charter persona dedupe <name>            # near-duplicate pairs
```

These only read. Done when you can say, for each finding, what you propose.

## 2. Compact, with the operator's yes

- **Exact duplicates and index drift** are safe and reversible: `--apply` moves a duplicate to
  `archive/` and repairs the index. Apply once the operator agrees.
- **Near duplicates** merge by hand: write the one memory that says both, then forget the rest.
- **Stale** memories are still true or they are not. Forget one only when the operator agrees it
  is no longer so.

```bash
charter workspace forget -w <name> <slug>
charter persona forget <name> <slug>
```

Done when every finding is applied, merged, kept on purpose, or declined by the operator.

## 3. Improve the charter

A lesson that has held across several memories, or that every chat in this workspace or persona
should act on from its first turn, belongs in the charter. Write it there in a sentence that
says what to do and why, then forget the memories it replaces.

For a persona, regenerate its sub-agent after editing `persona.md`:

```bash
charter persona sync-agents
```

Show the operator the charter's diff. Done when they have read it. Both files are committed, so
the change travels with the plane's next save.
