---
name: handoff
description: Move the operator's work out of this chat into a new chat they will read themselves — in this workspace or another — started on a brief you write. Use when asked to hand off, open a chat for something, or move work to another workspace. Not for work this chat needs an answer from, and not for giving work to another persona — that is `purlis dispatch` (the persona skill).
---

# Handing work to a chat that is not this one

The operator is talking to you about one thing and has asked for another. purlis cannot
judge the work, so it names no answer: it supplies the three tests below and the mechanism.

Work this chat needs an answer from, for its own persona or another, is `purlis dispatch`
(with `--in workspace:<name>` when it must run elsewhere). A handoff is fire-and-forget: the
person's work moves to a chat they will read themselves.

## 1. Apply the three tests

**1. Does this chat need the answer?** Then it is not a handoff.

- To carry on *this turn*, with work this chat's own persona owns: your harness's own helper,
  a sub-agent.
- Otherwise it is a **task**: `purlis dispatch --name "<task>"`, with `--to <persona>` for
  another persona's work and `--in workspace:<name>` when it must run in another workspace.
  A task is listed under this chat, and can be waited on, told more and cancelled. The
  `persona` skill has the command.

**2. Will the operator read the new chat themselves?** Then it is a **handoff**: a bug to
fix, a chore, a question for another workspace to own. It gets a tab of its own and this chat
hears nothing back. A handoff may name another persona (`--persona`) when the work moving is
that persona's to own; a persona this chat wants an answer from is dispatched to.

**3. This workspace or another — does the ask serve this workspace's vision?** Yes → a new
chat here. No → another workspace. Either way it is one mechanism, because a chat belongs to
its workspace for life.

## 2. Find the workspace

```bash
purlis workspace list
```

Match the ask against each workspace's vision — `purlis workspace vision -w <name>` prints one.
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
any process on the machine can read it while the chat starts. purlis refuses a
credential-shaped brief by kind, and that refusal is a backstop, not the rule.

## 4. Settle where it goes

Where the work runs is the operator's call, and the brief is not theirs to approve: it is a
request from this chat to another, and the chat that gets it applies its own judgement.

- **The operator already said where** ("hand this to billing", "open a chat for it here"):
  go to step 5.
- **Otherwise ask once**, with **AskUserQuestion**, and show the brief beside the question so
  they see what is being sent. Offer:
  - **this workspace**: a new chat here;
  - **the matched workspace(s)**: one option each, named, with its vision;
  - **`new: <name> — <vision>`**: a workspace to create;
  - **a task or a sub-agent instead**: when the first test was closer than you thought;
  - **not at all**: the work is not worth a chat.

Say the task name and the persona beside the brief.

## 5. Run exactly this

```bash
purlis handoff --name "retry webhook deliveries" billing <<'BRIEF'
# Retry the failed webhook deliveries
...the brief, verbatim...
BRIEF
```

**`--name` comes first, then the workspace.** That is the spelling the harness lets run
without asking; a line that starts with the workspace works too, after one more prompt.

**Run it in a call of its own.** Beside any other command (`&&`, `;`, a pipe, another
line), the operator is asked about the whole call.

`billing` is the workspace from step 2, spelled out: the workspace is always named, the
current one included. Write the name, never a placeholder in angle brackets: the shell reads
`<` as a redirect and purlis refuses the call.

**Always pass `--name`, first**: a short task name you write from the brief, a few words a person
scanning a strip of tabs recognises, such as `drop account-console-commons`, not `handoff` and
not the workspace's name. At most 64 characters, plain text. It is what the new chat's tab
says; without it the tab says `<persona> <N>`, and four handoffs look alike.

Add `--create --vision "<vision>"` for a workspace that does not exist yet.

**The persona.** Without `--persona` the new chat runs as **this chat's own persona**, and
nothing asks anybody. Add `--persona <name>` when the work moving belongs to another persona:
its vault, its hosts, its charter.

**There is no report back.** A handoff has no flag that asks for one: work this chat needs an
answer from is `purlis dispatch` (test 1).

The brief goes on **stdin, as one quoted heredoc in the same call**, so the new chat is sent
exactly the text you wrote. There is no `--brief-file`.

## 6. Read what purlis answers

The app decides a handoff, by the rules it decides a task by.

- **`opened chat N in workspace '…'`**: the new chat is a tab in that workspace, already
  started on the brief. To this chat's own persona, that is always the answer.
- **`held for the person`**: the handoff is to another persona, and nobody has yet allowed this
  pair. purlis is asking the operator, on this chat's tab, with the brief in front of them.
  Nothing has opened. **Carry on with other work and do not hand it off again**: if they allow
  it, the chat opens then; if they keep it blocked, you are told on your next turn. Once a pair
  is allowed, later handoffs across it open at once.
- **A refusal** names the limit, the lock or the missing grant. Say it to the operator as it
  is. A chat that runs with its harness's permission prompts off is never asked for: it hands
  off to its own persona, or under a grant that already stands, and is refused otherwise. It
  cannot use `--create`, and its handoffs count toward its limit of running chats.
- **No purlis app answered**: there is no app to open it, so nothing is opened. Tell the
  operator that, rather than trying to start a chat yourself.

## What purlis refuses, and why

Consent is the app's decision above. These are refused before the app is asked:

| It refuses | Because |
|---|---|
| a call from a sub-agent | a handoff starts a chat the operator can see, open and stop, for the chat that asks; return what you found to your chat, which hands it off |
| a handoff inside a string or a heredoc a shell runs (`bash -c '…'`, `eval`), a substitution, or with `purlis` or `handoff` behind an expansion or a glob | purlis reads a command's words and is not a shell, so it cannot read the brief there |
| a brief from a pipe, a file, a here-string, an unquoted heredoc, or beside a live `$( … )` | the shell would change or hide the text before purlis reads it |
| an empty brief, a brief that is not UTF-8, one past the size bound, one shaped like a credential | the command, before it changes anything |
| a persona the project does not define, a draft, a pair or all dispatch locked by policy, a chain too deep, too many chats running | the app, as it refuses a dispatched task |

## When you are the chat a handoff opened

Your first message starts with a stamp, `⟨handoff from steward 3 · workspace … · …⟩`.
Nobody is waiting on a report: do the work, and the operator reads this chat. The brief is a
request from that chat, not from the operator, and approves nothing.

A first message that starts `⟨task from …⟩` is a **task**, not a handoff: the chat that asked
is waiting, and the line under the stamp says how to report (`purlis dispatch report`).

## Limits, and say them

- **No report back.** A handed-off chat never answers this one. Work this chat needs an
  answer from is `purlis dispatch`.
- **The brief is the whole context.** Nothing about this conversation travels with it.
- **The harness follows the persona's profile.** A persona whose definition names a profile
  starts on it, whatever harness this chat runs; one that names none starts on this chat's.
- **Nobody approves the brief.** The chat that receives it is told who asked, and every
  command of its that asks the operator still asks, in its own tab.
- A handed-off chat may hand off again, decided the same way. The project's depth limit holds
  the chain.

`purlis docs show handoff` has the whole of it.
