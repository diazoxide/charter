---
name: smart-close
description: Close this chat by writing its session record — a summary of this session into its workspace's sessions/ — and folding what lasts into workspace.md. Use when asked to smart close, to wrap up or close this chat with a record, or to write a session record.
---

# Smart close

The chat is closing. Leave behind a **session record**: a summary the next chat reads to pick up
where this one stopped. A summary, never the transcript. The last command below is what closes
the tab, so run it last, once.

## 1. Review the session

Read back over this conversation. Collect, for the record:

- **Goal** — what this session set out to do.
- **Done** — what changed: files, commits, pull requests, decisions carried out.
- **Decisions** — what was decided, and why.
- **Open** — what is unfinished, blocked or still to ask.
- **How to resume** — the first thing the next chat does, and where.

Keep credentials out: name the vault a secret lives in, never its value. Done when you can write
each of the five in a few lines.

## 2. Fold what lasts into workspace.md

In a workspace, open its `workspace.md`. Move what should outlive this session into the section
it belongs to:

- a decision every later chat should act on → `## Context & decisions`;
- a term this work coined → `## Glossary`;
- a changed goal → `purlis workspace vision "<the goal>"`.

Leave `## Sessions` alone; that line is purlis's. At the plane root there is no `workspace.md`; a
lesson for every persona goes to `purlis persona remember <persona> --shared "<fact>"`. Done when
each lasting decision, term or goal change is in its section, or there was none.

## 3. Write the record

Title it in one line. Name the pieces this session worked in that you are not standing in, as
`<repo>/<piece>`; purlis reads their branches from git. The body is exactly these five sections,
in this order, each with at least one line:

```markdown
## Goal

<what this session set out to do>

## Done

<what changed>

## Decisions

<what was decided, and why>

## Open

<what is left; "Nothing." if nothing>

## How to resume

<the first step for the next chat>
```

Call purlis's `session_record` tool with `title`, `body` and, where there are any, `pieces`.
purlis writes the record for this chat, so it works from a sandboxed chat and from a clone. A
harness without purlis's tools runs the command instead, with the same title and body:

```bash
purlis session record --title "<one-line title>" --piece <repo>/<piece> <<'RECORD'
<the five sections>
RECORD
```

A refusal says what to fix: fix it and write the record again. Done when the answer names the
record's path. Then stop: when the operator started this close, the app closes this tab once this
turn ends. When the answer says the tab will not close by itself, tell the operator the record is
written and the tab is theirs to close.
