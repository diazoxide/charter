# A chat id names one chat for the life of the plane, and one harness session

## The failure

`state.new_chat_id` handed out the lowest free ordinal, and a reap freed an ordinal with its
directory. Two things keyed by the id live outside that directory — the captured
`<id>.transcript` and the quit record's entry — so a new `default.1` was offered an unrelated
chat's scrollback, and closing it deleted the earlier chat's record (#1101). Separately,
charter knew which conversation a chat held only for Claude Code, only after its first hook,
and lost it with the reap.

## The decision

1. **A chat id is handed out once per plane.** The allocator starts above the highest ordinal
   this plane has handed out for the prefix (`.charter/frame/chat-ids.json`, under
   `chat-ids.lock`) and above every ordinal a leftover still carries — chat directories,
   transcripts, every `chat` in `reopen.json` read raw, and `.charter/sessions/<id>.*`
   markers. The mark is written before the claim, so no id leaves unrecorded, and a mark that
   cannot be written hands out nothing. The `mkdir` still decides who wins. A restored chat
   keeps its id: a reopen claims exactly the recorded directory, and a directory still
   standing there is proven dead by one listing on the chat's own server before it is reaped —
   a server that did not answer proves nothing.
2. **A tab is linked to exactly one harness session, never guessed.** Charter chooses the id
   where the harness takes one (Claude Code's `--session-id <uuid> --name <chat id>`), and
   records the id the harness reports where it does not (Codex at SessionStart, opencode at
   its first tool hook). The id and the name are added by the launcher at the `exec`, from the
   chat's own record, and never cross tmux. Only the chat's own harness may change the link —
   a report from a harness nested inside the chat changes nothing.
3. **Resume is offered only when the conversation exists** — the transcript the harness named
   is a file, or, for opencode, which names none, a hook has reported the session.
4. **A title is display; the id is identity.** Claude Code's session name is composed from both
   at every `exec` (the title arrives with a later task; until then the name is the id).

## Considered and rejected

- *The lowest free ordinal, with every leftover deleted at the reap* — a leftover charter does
  not yet know about is inherited the day it is written; never reusing the name needs no list.
- *A hash id* — `new_chat_id`'s own docstring: a truncated hash collides silently.
- *Ids renamed to titles* — every `$CHARTER_SESSION_ID` already exported into a live process
  would name nothing.
- *`--continue`, or "the newest session in this directory"* — two chats of one workspace share
  a directory, so that is the guess this ADR exists to refuse.
- *Any report carrying the chat's variables* — a `claude -p` in the chat's own shell carries
  them too.
- *`os.getppid() == $CLAUDE_PID`* — measured unreliable: Claude runs hooks under `/bin/sh -c`,
  and a compound hook command keeps one or two shells in between.
- *Typing `/rename` or `/resume` into the harness* — ADR 0018.
- *`codex resume <name>`* — Codex has no flag to set a name (openai/codex#14482), so a name
  charter did not set cannot identify a session.

## Measured, 2026-09-15

claude 2.1.272 run live; codex-cli 0.147.0 and opencode 1.18.23 read from their tagged source
(`rust-v0.147.0`, `v1.18.23`) and not run; tmux 3.7c.

- Claude Code reports the chosen id at SessionStart, but writes no transcript until the first
  prompt (C1, C2).
- A resume with `--resume <id> --name <name>` reports the same id and renames the session
  (C3). `--resume` together with `--session-id` is refused (C4).
- A nested `claude` reports a new id from its own `CLAUDE_PID` (C5).
- `/clear` reports a new id from the same `CLAUDE_PID` (C6).
- Every Claude Code hook's `CLAUDE_CODE_SESSION_ID` equals its payload's `session_id`, at
  startup and after `/clear`, while a hook's parent is Claude only for a lone command (C7).
- Codex reports at its first turn, not at launch; `codex resume <id>` looks that uuid up
  exactly; a resumed root session reports the id it had (X1–X3).
- opencode's tool hook carries `sessionID`, and `opencode -s` looks the id up in the working
  directory (O1, O2).

## Consequences, including what they cost

- One more file and a lock under `.charter/frame/`, and ordinals only grow: a prefix hands out
  at most 99,999 ids in the life of a plane (`chats._MAX_ORDINAL_DIGITS`).
- A Claude Code report counts only when `CLAUDE_CODE_SESSION_ID` equals its payload's id, and
  its `CLAUDE_PID` either becomes the adopted pid through the recorded id or equals it. The
  adopted pid comes from the report, never from `#{pane_pid}`, because a wrapper profile's
  pane pid is the wrapper's. A nested non-Claude harness is rejected by inference, not by
  measurement.
- A Codex report is recognised by the exact key set Codex declares for SessionStart; a Codex
  that adds a field is recognised as no report, and that chat is offered no resume until
  charter is updated.
- The link is as trustworthy as the hook that writes it: a process that fakes the payload,
  `CLAUDE_CODE_SESSION_ID` and `CLAUDE_PID` together can point a tab at another conversation.
- Claude Code's link follows `/clear`, so resume offers the conversation the operator is in;
  the pre-clear conversation is still in `claude --resume`'s own picker, but charter stops
  offering it.
- Codex and opencode carry no pid a hook can compare, so each start adopts its first reported
  id: after `/new` inside Codex, or a new session inside opencode, resume offers that start's
  first conversation.
- A Codex chat that has not taken a turn has no link, and a Claude Code chat with no prompt has
  no transcript, so neither is offered resume.
- An opencode conversation that never ran a tool reports no session, and one whose chat is
  reopened in another directory reopens empty.
- A reopened chat whose directory is held by a live pane of this plane, or whose server did not
  answer, is not reopened; it stays recorded, and the sentence names what clears it.
- A rename reaches Claude Code only at its next start or resume, and Codex and opencode keep
  their own session names.
