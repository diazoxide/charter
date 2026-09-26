---
name: safe-remove
description: Remove a charter workspace or persona without losing what it learned — audit it, promote durable learnings to shared memory, a persona's memory or the plane's docs, then run the guarded remove. Use when asked to remove, delete, retire or clean up a workspace or a persona.
---

# Safe remove

A workspace or a persona holds lessons nobody wrote anywhere else: its memory, its charter,
its todos, the work in its repos. Removing it deletes all of that. Safe remove is three steps,
in order, and the operator sees each one before the next.

You run at the plane root, because the directory being removed is going away.

## 1. Audit

Read everything the subject holds, and sort each thing into **promote**, **leave** or **ask**.

A workspace:

```bash
charter workspace recall -w <name>       # its memory, oldest first
charter ws todo -w <name>                # its open todos
charter status -w <name>                 # its repos: uncommitted, unpushed, open pieces
```

and its `workspace.md`.

A persona:

```bash
charter persona show <name>              # its charter
charter persona recall <name>            # its memory
charter persona curation list <name>     # the curation actions it declares
```

Done when every memory, every open todo and every repo with unpushed work has a verdict.

## 2. Promote

A **durable learning** is one that is still true and still useful after the subject is gone: a
decision and its reason, a gotcha, a verified fact about a system. Write each where it will be
found:

```bash
charter persona remember --shared "<fact>"       # every persona needs it
charter persona remember <persona> "<fact>"      # one persona owns the domain
```

or the plane's docs, when it describes the plane itself. A todo that still matters moves to the
workspace that will do it (`charter ws todo -w <other> "<text>"`). A repo's unpushed work is
the operator's call: name it and ask.

Show the operator what you promoted, what you are leaving behind and why. Done when they have
said go.

## 3. Remove

```bash
charter workspace remove <name>
charter persona remove <name>
```

Each one guards what it would destroy: `workspace remove` refuses while a clone or a piece holds
work nothing else has (exit 2), and `persona remove` refuses while another persona extends or
uses the one being removed. A refusal is the answer: relay it in its own words and stop.
`--force` belongs to the operator, and you pass it only when they say so.

Removing a persona deletes its whole directory, memory and curation actions with it. The
deletion is a change to the plane like any other and travels with its next save.
