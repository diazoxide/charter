---
version: unreleased
headline: A new chat is never given a closed chat's id — so it no longer inherits that chat's transcript or deletes its quit record — and Codex and opencode chats now resume
---

A chat id used to be the lowest number free in its workspace, so closing `default.1` and
opening a new chat made another `default.1` (#1101). Two things keyed by the id outlive the
chat: its captured transcript and its entry in the quit record. So the new chat was offered
the old chat's scrollback under `chat: previous transcript`, and closing it deleted the old
chat's record, which `charter reopen` could still have restored.

**Ids only grow now.** Each workspace's highest id is remembered in the plane, and a new chat
starts above it and above anything an earlier chat left on disk. Existing chats keep their
ids. **A reopened chat keeps its id too**, instead of coming back under a new one. If that
id's directory is still held by a running pane, or sits on a tmux server that does not
answer, the chat is not reopened; it stays recorded, and the line names what clears it.

**Every tab is linked to exactly one harness session.** Claude Code is started with an id
charter chooses and the chat's id as its name, and follows `/clear`; a `claude` run from the
chat's own shell does not move the link. Codex and opencode are linked to the first session
they report in each start, and now resume too: `codex resume <id>` and `opencode -s <id>`.
Resume is offered only when there is a conversation to resume.

The limits, per harness, are in `docs/harnesses.md`: a Claude Code chat with no prompt yet has
nothing to resume; Codex reports its session at the first turn, and after `/new` resume offers
that start's first conversation; opencode resumes only in the directory the chat ran in. The
Codex and opencode behaviour was read from their source at codex-cli 0.147.0 and opencode
1.18.23, not run.
