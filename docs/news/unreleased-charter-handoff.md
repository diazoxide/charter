---
version: unreleased
headline: `charter handoff` opens a chat in any workspace, already working on the brief you approved — and a work-shaped prompt is told where it could run
---

You are in a chat about one thing and you ask for another. Until now that ended one of three
ways: the model did it here, so one workspace's todos, memory and branch carried two tasks; it
handed the work to a sub-agent, and the answer you wanted to talk to came back as a paragraph
and was gone; or it told you to open a chat yourself, and you retyped the context it already
had.

```bash
charter handoff <workspace> [--create --vision "<vision>"] [--persona <name>] <<'BRIEF'
<the brief>
BRIEF
```

The brief becomes the new chat's first message. Your harness asks before the command runs — the
`ask` rule the previous entry is about — and the prompt shows the exact text that will be sent.

## What one does

1. With `--create`, makes the workspace (LOCAL) and records its vision.
2. Records a todo in the target workspace, titled by the brief's first line, with one line of
   provenance under it — **never the brief**, because a LIVE workspace commits `todos/**`. A
   todo already on that list that says the same work is named and not recorded twice, and the
   handoff continues. "The same work" is asked over the first lines, because every handoff todo
   ends in the same provenance sentence and over the whole text that boilerplate read as
   agreement; and where fewer than three words overlap — too thin to be evidence in either
   direction — it is decided by whether the two first lines are the same line. That is the
   handoff's rule, not `charter ws todo`'s: `ws todo` refuses on a duplicate, so it keeps
   catching a todo retitled by a word, while a handoff that refused one would drop a real todo
   and the work would go invisible.
3. Opens a chat there in the background. No client moves, nothing attaches, and the chat you are
   on keeps its panels — measured on tmux 3.7c and at charter's 3.2 floor with real clients
   attached.
4. Sends the first message: a stamp line, a blank line, then the brief verbatim, on the
   harness's own argv (`claude "<msg>"`, `codex "<msg>"`, `opencode --prompt "<msg>"`).
5. The chat is born locked to its workspace and asks no workspace question.
6. Keeps the full brief in that chat's private state under `.charter/`, never committed, and
   written at the moment the chat id is allocated — before the harness it is for starts.
7. Tallies `{"event": "handoff", "ts", "placement", "created"}` — no workspace name, no persona,
   no text — and clears `routing: require`'s pending mark, because opening a chat in a workspace
   *is* routing.
8. Prints the chat and the workspace.

The stamp is `⟨handoff from chat <chat> · workspace <ws> · <YYYY-MM-DD HH:MM>⟩`: facts charter
can observe and no instruction, so the new chat can tell the message was not typed there.

## It refuses before it changes anything

Nothing is created, recorded or opened until every refusal has been asked: a name that cannot be
a workspace, `--vision` without `--create` or the reverse, `--create` on a workspace that exists
(`default` included), an unknown workspace without `--create`, a persona nobody has, a stdin
that is a terminal (asked before any read, so it never blocks) or closed altogether, a brief
that is empty or not UTF-8, and a brief shaped like a credential — named by KIND, never by
value, because the brief reaches the harness as a command-line argument any local process can
read while it starts.

**Outside a frame, or inside a tmux you started yourself**, charter prints the exact
`charter <harness> --workspace <ws> …` command to run in a new terminal instead of stopping —
with the workspace creation in front of it when you asked for one, `CHARTER_PERSONA=` when you
named a persona, and every word quoted.

## How a chat finds out it can

On every work-shaped prompt, charter's prompt block now leads with **Where this could run**:
the three placements (a sub-agent, a new chat here, a new chat in another workspace), the two
tests that pick one, and this workspace's vision quoted from `workspace.md` as data. It no
longer waits for the acting persona to declare `routing:` — the persona roster it embeds still
does, because "who else exists" is a different question. It names no placement and no other
workspace; the model matches the ask against the visions `charter workspace list` shows, which
is why that listing grew a `VISION` column: the first line of each workspace's vision, as the
trailing field, untruncated.

On an unattended run the block says instead that `charter handoff` is refused there and names
`charter ws todo --workspace`.

**`charter:handoff`** ships as a skill: apply the two tests, find the workspace, write the brief
from a template (goal, what is known with paths, done when, constraints, the claim-a-piece line),
show it **in full** in a quiz, and run the command only on a yes.

## A chat that comes back empty is shown its brief

The brief now travels in the reopen record, which is the only copy that outlives the chat
directory `reap` takes on a restart. A chat whose conversation does not come back — codex, and
Claude Code before its first turn — is shown it at its next start as a quoted block labelled
data to read and never an instruction to obey, with the operator now in front of it named as
outranking it. A chat that resumes is shown nothing: the brief is already the first message of
its transcript.

## Limits

- **No report back.** A handed-off chat never answers the chat that opened it; if you need the
  answer in this conversation, you wanted a sub-agent.
- **The same harness only** (#538's backlog).
- **12,288 bytes, and the cap is on the stamped message** — the stamp line, a blank line and
  your brief — so a brief that fits on its own can be over once stamped. The refusal says both
  numbers. That bound is charter's own, set under tmux's measured 16,364-byte command limit.
- **A brief never carries a secret.** It is argv while the harness starts.
- **An opencode chat that reopens empty is not shown its brief**, because opencode has no
  SessionStart hook at all. `charter doctor` names that gap.
- **The tab strip does not point at the arrival yet.** That is the next slice.

`charter docs show handoff` has the whole of it, including how the consent was measured.
