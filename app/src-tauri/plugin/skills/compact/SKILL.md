---
name: compact
description: Compact and improve a purlis workspace's or persona's memory — find duplicates and stale entries with purlis's optimize and dedupe reports, prune only with the operator's yes, and fold durable lessons into workspace.md or persona.md. Use when asked to compact, tidy, prune, dedupe or improve a workspace's or persona's memory.
---

# Compact & improve

Memory grows by one file per lesson, and nothing merges it. Compacting makes it smaller and
truer; improving moves what has become a rule out of the journal and into the charter every
chat reads first (`workspace.md` for a workspace, `persona.md` for a persona).

## 1. Read the reports

A workspace:

```bash
purlis workspace optimize <name>        # exact duplicates, near duplicates, stale, index drift
```

A persona:

```bash
purlis persona optimize <name>
purlis persona dedupe <name>            # near-duplicate pairs
```

These only read. Done when you can say, for each finding, what you propose.

## 2. Compact, with the operator's yes

- **Exact duplicates and index drift** are safe and reversible: `--apply` moves a duplicate to
  `archive/` and repairs the index. Apply once the operator agrees.
- **Near duplicates** merge by hand: edit one of them to say both, then archive the rest.
- **Stale** memories are still true or they are not. A memory that is half true is edited in
  place; one that is no longer so is archived, and only when the operator agrees.

```bash
purlis workspace edit -w <name> <slug> [--title "<title>"] ["<body>" | -]
purlis persona edit-memory <name> <slug> [--title "<title>"] ["<body>" | -] [--shared]
purlis workspace archive -w <name> <slug>              # undo: workspace unarchive
purlis persona archive-memory <name> <slug> [--shared] # undo: persona unarchive-memory
purlis workspace move-memory -w <name> <slug> --to-shared  # or --to-persona <p>
purlis persona move-memory <name> <slug> [--shared] --to-workspace <ws>
```

Each move takes exactly one of `--to-workspace <ws>`, `--to-persona <p>` and `--to-shared`.

An edit keeps the memory's slug and date. An archive moves it to `memory/archive/`, out of every
list, and `unarchive` / `unarchive-memory` puts it back. `workspace forget` and `persona forget`
delete for good; keep them for a memory that must not survive, such as one holding a secret.
A memory in the wrong scope, such as a workspace lesson every persona should know, is moved, not
copied: the move keeps its title and date and prints the command that moves it back.

Done when every finding is applied, merged, kept on purpose, or declined by the operator.

## 3. Improve the charter

A lesson that has held across several memories, or that every chat in this workspace or persona
should act on from its first turn, belongs in the charter. Write it there in a sentence that
says what to do and why, then archive the memories it replaces.

Show the operator the charter's diff. Done when they have read it. Both files are committed, so
the change travels with the plane's next save.
