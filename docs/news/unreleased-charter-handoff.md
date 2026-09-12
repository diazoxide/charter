---
version: unreleased
headline: `charter handoff` opens a chat in any workspace, already working on the brief you approved
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
   todo already on the list is reported and not recorded twice, and the handoff continues.
3. Opens a chat there in the background. No client moves, nothing attaches, and the chat you are
   on keeps its panels — measured on tmux 3.7c and at charter's 3.2 floor with real clients
   attached.
4. Sends the first message: a stamp line, a blank line, then the brief verbatim, on the
   harness's own argv (`claude "<msg>"`, `codex "<msg>"`, `opencode --prompt "<msg>"`).
5. The chat is born locked to its workspace and asks no workspace question.
6. Keeps the full brief in that chat's private state under `.charter/`, never committed.
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
that is a terminal (asked before any read, so it never blocks), a brief that is empty or not
UTF-8, and a brief shaped like a credential — named by KIND, never by value, because the brief
reaches the harness as a command-line argument any local process can read while it starts.

**Outside a frame, or inside a tmux you started yourself**, charter prints the exact
`charter <harness> --workspace <ws> …` command to run in a new terminal instead of stopping —
with the workspace creation in front of it when you asked for one, `CHARTER_PERSONA=` when you
named a persona, and every word quoted.

## Limits

- **No report back.** A handed-off chat never answers the chat that opened it; if you need the
  answer in this conversation, you wanted a sub-agent.
- **The same harness only** (#538's backlog).
- **12,288 bytes, and the cap is on the stamped message** — the stamp line, a blank line and
  your brief — so a brief that fits on its own can be over once stamped. The refusal says both
  numbers. That bound is charter's own, set under tmux's measured 16,364-byte command limit.
- **A brief never carries a secret.** It is argv while the harness starts.
- **The tab strip does not point at the arrival yet**, and a chat that reopens empty is not
  shown its brief. Both are the next slices.

`charter docs show handoff` has the whole of it, including how the consent was measured.
