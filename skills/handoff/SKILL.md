---
name: handoff
description: Hand a request that does not belong in this chat to a new chat — in this workspace or another — that starts working on a brief the operator approved. Use when a request should run somewhere other than this conversation, or when asked to hand off, open a chat for something, or move work to another workspace.
---

# Handing work to a chat that is not this one

The operator is talking to you about one thing and has asked for another. There are three
places that second thing can run, and charter names none of them for you — it cannot judge
the work (`docs/adr/0016`). It supplies the facts, the two tests below, and the mechanism.

1. **A sub-agent** — your harness's own. charter never touches one.
2. **A new chat in this workspace.**
3. **A new chat in another workspace**, existing or new.

2 and 3 are one mechanism, a **handoff**, because a chat belongs to its workspace for life.
The only thing that differs is the workspace.

## 1. Apply the two tests

**Sub-agent or chat — who reads the result?** If this chat needs the answer to continue, it
is a sub-agent. If the operator will read it and talk to it, it is a chat. There is no
report-back channel from a handed-off chat: a parent that needs the answer wanted a
sub-agent, and a handoff would leave it waiting for a message that never comes.

**This workspace or another — does the ask serve this workspace's vision?** Yes → a new
chat here. No → another workspace.

## 2. Find the workspace

```bash
charter workspace list
```

The VISION column is what you match the ask against. The rules for the *proposal* — the
command refuses none of them:

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

If you will write in a repo, claim your own piece first: charter wt add <repo> <piece>.
```

The first line becomes the todo charter records in the target workspace, so make it a
title somebody scanning that list would recognise.

**A brief never carries a secret.** It reaches the harness as a command-line argument, so
any process on the machine can read it while the chat starts. charter refuses a
credential-shaped brief by kind, and that refusal is a backstop, not the rule.

## 4. Ask, with the brief on screen

Quiz with **AskUserQuestion**, showing the brief **in full** — not a summary of it. The
brief becomes another chat's first message and runs with the operator's authority; an
approval of a summary is not an approval of the text that gets sent. Offer:

- **this workspace** — a new chat here;
- **the matched workspace(s)** — one option each, named, with its vision;
- **`new: <name> — <vision>`** — a workspace to create;
- **a sub-agent instead** — when the first test was closer than you thought;
- **not at all** — the work is not worth a chat.

## 5. On a yes, run exactly this

```bash
charter handoff billing <<'BRIEF'
# Retry the failed webhook deliveries
...the brief, verbatim...
BRIEF
```

`billing` is the workspace from step 2, spelled out: the workspace is always named, the
current one included, because the permission prompt has to say where the chat goes and `.`
says nothing. Write the name, never a placeholder in angle brackets — the shell reads `<`
as a redirect and charter refuses the call.

Add `--create --vision "<vision>"` for a workspace that does not exist yet, and
`--persona <name>` when the quiz named one. Both are visible in the prompt.

The brief goes on **stdin, as one quoted heredoc in the same call**. That is what makes the
permission prompt show the exact text the new chat is sent. There is no `--brief-file`: a
prompt that shows a path is an approval of a path.

## What charter refuses, and why

The prompt your harness raises in front of `charter handoff` **is** the consent, so charter
refuses every shape that prompt cannot stand in front of. Each one is the rule working:

| It refuses | Because |
|---|---|
| any spelling but `charter handoff` — `python3 -m charter handoff`, a path, `charter 'handoff'` | measured: the host's rule does not match those, so no prompt appears |
| a brief from a pipe, a file, a here-string, or a heredoc a shell runs | the prompt would show a path or a `bash`, not the brief |
| a call from a sub-agent | there is no operator in a sub-agent's turn to answer the prompt |
| an unattended run (`bypassPermissions`) | the same, and nothing would ask |
| outside a frame, or inside a tmux you started yourself | nothing can open a chat in the background there — charter prints the command to run in a new terminal instead |
| an empty brief, a brief that is not UTF-8, one past the size bound, one shaped like a credential | the command, before it changes anything |

The sub-agent and unattended refusals read the harness's own hook payload, and not every
harness sends one: opencode's carries neither `agent_id` nor `permission_mode`, so neither
is refused there. `charter doctor` names that gap rather than charter pretending to enforce
it. `charter docs show handoff` has the measurements, per harness.

## Limits, and say them

- **No report back.** The new chat never answers this one.
- **The brief is the whole context.** Nothing about this conversation travels with it.
- **The same harness.** A Claude Code chat hands off to a Claude Code chat.
- A handed-off chat may hand off again, under the same prompt. There is no depth limit,
  because every hop needs its own yes.

`charter docs show handoff` has the whole of it, including how the consent was measured.
