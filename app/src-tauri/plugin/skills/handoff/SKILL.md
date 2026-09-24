---
name: handoff
description: Hand a request that does not belong in this chat to a new chat — in this workspace or another — that starts working on a brief the operator approved. Use when a request should run somewhere other than this conversation, or when asked to hand off, open a chat for something, or move work to another workspace.
---

# Handing work to a chat that is not this one

The operator is talking to you about one thing and has asked for another. There are three
places that second thing can run, and charter names none of them for you — it cannot judge
the work. It supplies the facts, the two tests below, and the mechanism.

1. **A sub-agent** — your harness's own. charter never touches one.
2. **A new chat in this workspace.**
3. **A new chat in another workspace**, existing or new.

2 and 3 are one mechanism, a **handoff**, because a chat belongs to its workspace for life.
The only thing that differs is the workspace.

## 1. Apply the three tests

**Sub-agent or chat — who reads the result?** If this chat needs the answer to continue *this
turn*, it is a sub-agent. If the operator will read it and talk to it, it is a chat.

**Fire-and-forget, or needs an answer?** Decide per handoff, and say which in the quiz:

- **Fire-and-forget** (the default) — the work stands on its own and the operator reads the
  new chat directly: a bug to fix, a chore, a question for another workspace to own.
- **Needs an answer** (`--report`) — this chat has to act on the outcome later: it is waiting
  on a fix to build on, a decision, a finding. The new chat reports back when it is done, and
  the report reaches this chat on its next turn. Do not ask for one out of habit: every report
  is an item on the operator's needs-you list.

**This workspace or another — does the ask serve this workspace's vision?** Yes → a new
chat here. No → another workspace.

## 2. Find the workspace

```bash
charter workspace list
```

Match the ask against each workspace's vision — `charter workspace vision -w <name>` prints one.
The rules for the *proposal* — the command refuses none of them:

- a workspace with **no vision** is never proposed; there is nothing to match against;
- `default` is never a target; it is where a plane lands when nobody chose, not a task;
- a workspace whose vision says it is **delivered** is proposed only when the ask reopens
  its task;
- **always also offer a new workspace**, with a name and a one-line vision. A workspace
  created without a vision is created unfindable.

## 3. Write the brief

The brief is the whole context the new chat gets. No pointer to this transcript, no forked
conversation — everything it needs is in this text, and there is nobody for it to ask.

```markdown
# <one-line goal>

**Goal** — what done looks like, in one paragraph.

**What is known** — the facts, with paths. `workspaces/<ws>/<repo>/<file>`, the issue
number, the command that reproduces it. Name files; do not paste them.

**Done when** — the check that settles it: a test that passes, a PR open, a question
answered in one line.

**Constraints** — what must not change, what has already been tried, what the deadline is.
```

Make the first line a title somebody scanning a strip of tabs would recognise.

**A brief never carries a secret.** It reaches the harness as a command-line argument, so
any process on the machine can read it while the chat starts. charter refuses a
credential-shaped brief by kind, and that refusal is a backstop, not the rule.

## 4. Ask, with the brief on screen

Quiz with **AskUserQuestion**, showing the brief **in full** — not a summary of it. The
brief becomes another chat's first message and runs with the operator's authority; an
approval of a summary is not an approval of the text that gets sent.

Say the task name and whether it reports back beside the brief. Offer:

- **this workspace** — a new chat here;
- **the matched workspace(s)** — one option each, named, with its vision;
- **`new: <name> — <vision>`** — a workspace to create;
- **a sub-agent instead** — when the first test was closer than you thought;
- **not at all** — the work is not worth a chat.

## 5. On a yes, run exactly this

```bash
charter handoff billing --name "retry webhook deliveries" <<'BRIEF'
# Retry the failed webhook deliveries
...the brief, verbatim...
BRIEF
```

`billing` is the workspace from step 2, spelled out: the workspace is always named, the
current one included, because the permission prompt has to say where the chat goes and `.`
says nothing. Write the name, never a placeholder in angle brackets — the shell reads `<`
as a redirect and charter refuses the call.

**Always pass `--name`**: a short task name you write from the brief, a few words a person
scanning a strip of tabs recognises — `drop account-console-commons`, not `handoff` and not
the workspace's name. At most 64 characters, plain text. It is what the new chat's tab says;
without it the tab says `<persona> <N>`, and four handoffs look alike.

Add `--report` when the second test said **needs an answer**.

Add `--create --vision "<vision>"` for a workspace that does not exist yet, and
`--persona <name>` when the quiz named one. Both are visible in the prompt.

The brief goes on **stdin, as one quoted heredoc in the same call**. That is what makes the
permission prompt show the exact text the new chat is sent. There is no `--brief-file`: a
prompt that shows a path is an approval of a path.

**What happens next.** Inside the charter app, the app opens the new chat as a tab in that
workspace, already started on the brief, and says so. Anywhere else there is no app to open
it, so nothing is opened: charter says to open the charter app and exits 1 — tell the
operator that, rather than trying to start a chat yourself.

## What charter refuses, and why

The prompt your harness raises in front of the handoff **is** the consent, so charter refuses
every shape that prompt cannot stand in front of. Each one is the rule working:

| It refuses | Because |
|---|---|
| any spelling but `charter handoff` — a path to the binary, `charter 'handoff'` | the host's permission rule does not match those, so no prompt appears |
| a brief from a pipe, a file, a here-string, or a heredoc a shell runs | the prompt would show a path or a `bash`, not the brief |
| a call from a sub-agent | there is no operator in a sub-agent's turn to answer the prompt |
| an unattended run (`bypassPermissions`) | the same, and nothing would ask |
| an empty brief, a brief that is not UTF-8, one past the size bound, one shaped like a credential | the command, before it changes anything |

## When you are the chat a handoff opened

Your first message starts with a stamp, `⟨handoff from steward 3 · workspace … · …⟩`. If the
line under it says the chat that handed this off wants an answer, then when the work is done —
or when you are stuck and cannot finish — finish with:

```bash
charter handoff report "Dropped account-console-commons from both repos; PRs #41 and #42 open.
One caller left in billing-ui, noted in its todos."
```

A few plain lines: what was done, where it is, what is left. No secrets, no pasted files —
name them by path. It goes back to the chat that asked, and only there; charter chose the
recipient when it opened this chat. **You get exactly one report**, so send it at the end,
not as progress notes; a second is refused whatever happens in between. If the chat that
asked needs another answer later, it hands off again with `--report`.

Without that line under the stamp, nobody is waiting on a report and `charter handoff report`
is refused — just finish the work.

## Limits, and say them

- **A report back only when asked.** Without `--report` the new chat never answers this one,
  and with it exactly one report arrives on this chat's next turn, not in the middle of
  this one. For a second answer, hand off again with `--report`.
- **The brief is the whole context.** Nothing about this conversation travels with it.
- **The same harness.** A Claude Code chat hands off to a Claude Code chat.
- A handed-off chat may hand off again, under the same prompt. There is no depth limit,
  because every hop needs its own yes.

`charter docs show handoff` has the whole of it.
