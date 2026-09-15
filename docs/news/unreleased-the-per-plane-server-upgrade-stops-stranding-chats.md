---
version: unreleased
headline: Four rough edges of the per-plane tmux server are smoothed — a tab on the old server names its own way back, Esc closes a selector on either server, a workspace tab joins its own live session, and a transcript viewer no longer strands a chat's session
---

Giving each plane its own tmux server split a frame's chats across two servers on the day you
upgrade — the frames that were already open stay on the old `charter` server, and every chat
opened since is on this plane's own `charter-plane-<hex>` one. That split left four rough
edges, all fixed here.

**A chat tab on the other server now says how to reach it.** A client cannot move between two
tmux servers, so pressing a tab for a chat that is still on the old server can only refuse.
The chat strip now marks such a tab (a `~` where an ordinary idle tab is blank), `F2 → chat`
says "on another server" on its row, and the refusal names the way back on rather than
stopping at a dead end: quit that frame with `charter frame-quit` typed in its project, and
`charter` brings its chats back on this plane's own server.

**Esc closes a new chat's profile selector again — on either server.** A selector opened for a
chat whose recorded server did not match where its pane actually was (an upgrade-day skew)
would leave the tab sitting there forever on Esc. Charter now proves the pane by the pid of
the process asking — one pane on the machine — across the servers a chat can be on, so Esc
finds and closes it whether the chat is recorded on the legacy socket or this plane's own.

**A workspace tab joins its own live session instead of refusing it.** Pressing a tab for a
workspace whose session is already running on this plane's own server used to refuse it as
"probably another plane's". On a per-plane server the only sessions are this plane's, so the
tab now focuses or joins that session (opening a chat in it if it has none). The
"another plane's" refusal is kept for the shared servers — the legacy socket and a tmux you
run yourself — where a session of that name really can be somebody else's.

**A transcript viewer no longer keeps a dead chat's workspace alive.** "Previous transcript"
opens a pager in its own window beside the chat. When the chat was closed, quit, or its
harness died, that window used to stay — a session holding nothing charter recognised, which
then read as another plane's and blocked reopening the workspace. The viewer now goes with the
chat it was opened for, and any viewer left over by a crash is swept before the next launch
reads what is live.
