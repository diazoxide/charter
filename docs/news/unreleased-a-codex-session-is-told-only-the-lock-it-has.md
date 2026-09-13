---
version: unreleased
headline: A Codex session outside a frame is no longer told its workspace is locked
---

A Codex session started outside a charter frame has no workspace lock. Charter said it had
one in two places, measured on 0.60.0:

- `charter harness list` and `charter doctor` described Codex's `session-lock` gap as
  **"the workspace lock falls back to the terminal-pane key"**. There is no such fallback.
- The session briefing told the agent that `charter workspace use <name>` **"locks the
  workspace for the session — it can't be switched mid-session"**. In that session a second
  `charter workspace use` switched without a word.

## Why there is no lock there

Charter writes the lock under the session's id, and Codex passes no per-session id to its
shell. `charter workspace use` in that shell remembers the workspace for the terminal, and
that is a selection, not a lock: it refuses nothing. 0.61.0 already stopped the command
itself from printing "🔒 locked for this session" there.

## What is true now

- **The gap says what it is:** *no workspace lock outside a frame*. Inside a frame the chat's
  id reaches the Codex shell, so a Codex chat is locked to the workspace it was launched in,
  like any other chat.
- **The briefing still asks for a workspace**, and leaves out the lock only where two
  things agree: it sees no session id, and the harness it is running under is one charter
  records as passing no session id to its shell. Today that is Codex.

Claude Code and opencode sessions, in a frame or not, are unaffected: their briefing still
says confirming locks the workspace, because it does. That holds even when the briefing
itself sees no session id, as in the context file `charter init` writes for opencode.
