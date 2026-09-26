---
name: add-curation-action
description: Add a curation action to a charter persona — a chat the operator can open on a workspace, a persona or the plane with a prompt already typed. Use when asked to add, write or change a persona's curation action, or to give a persona a new entry in the Curate menu.
---

# Adding a curation action

A **curation action** is a chat the operator opens from a workspace, a persona or the plane,
with a prompt already typed into it. The operator reads the prompt and presses Enter; nothing
sends it for them. The persona that declares the action is the one that runs it.

Each is one file, `personas/<persona>/curation/<id>.md`:

```markdown
---
label: Review open work
on: workspace
runs-in: subject
---

Review the open work in the workspace `{subject.name}` ({subject.path}) ...
```

- `label` — what the menu shows. Required, one line.
- `on` — the kinds of subject it is offered on: `workspace`, `persona`, `plane`, comma-separated.
  Required.
- `runs-in` — `subject` (the workspace's directory, `personas/<name>/`, or the plane root) or
  `plane`. Without it, a workspace action runs in the workspace and the others at the plane
  root.
- The body is the prompt. It may use exactly four variables: `{subject.kind}`,
  `{subject.name}`, `{subject.path}` and `{plane.root}`. They are filled in as plain text;
  any other `{word}` is an error.

## 1. Ask what it is for

Ask the operator what the chat should do, which subjects it belongs on, and where it should
run. Draft the prompt as the operator would want to read it before pressing Enter: plain
language, and when it relies on a skill, name the skill in words. Done when the operator
approves the label, the kinds and the prompt.

## 2. Write it

```bash
charter persona curation add <persona> <id> --label "<label>" --on workspace,persona [--runs-in plane] <<'PROMPT'
<the prompt>
PROMPT
```

`<id>` is lowercase letters, digits, `.`, `_` and `-`. The command writes the file only when it
reads back with no error, and never over one that is there: to change an action, remove it and
add it again.

```bash
charter persona curation remove <persona> <id>
```

charter's own actions are `charter/safe-remove`, `charter/compact` and
`charter/add-curation-action`. A persona's action with one of their ids or labels is refused,
so choose another.

## 3. Check it

```bash
charter persona curation list <persona>
charter curation show workspace:<name>     # or persona:<name>, or plane
charter persona lint <persona>
```

`curation show` prints what a subject is offered, in order, each with who runs it, where, and
the prompt as it will be typed. Done when the new action is listed there with no warning. The
file is committed with the persona and travels with the plane's next save.
