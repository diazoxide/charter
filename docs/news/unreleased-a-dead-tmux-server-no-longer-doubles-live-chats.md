---
version: unreleased
headline: `charter reopen` no longer starts a second copy of running chats when one of the plane's tmux servers is gone
---

Since 0.62.0 a plane's chats can be on two tmux servers at once: the plane's own, and the
old shared `charter` server for a chat started before the upgrade. When one of them did not
answer, because it was killed with `tmux kill-server`, crashed, or was never started,
`charter reopen` and the restore bare `charter` does read the whole plane as stopped. They
then started a second copy of every chat still running on the other server, with a new id,
so nothing on screen told the copies apart.

Now each server is asked on its own. A chat running on a server that answers means the plane
is running, and the reopen is refused as before:

    charter reopen: this plane is already running — reopening it would open a second copy of every chat, …

A server that does not answer and has nothing listening on its socket is running no chats,
so the chats recorded on it come back. A server that does not answer but is still there,
such as a hung tmux, may still be running its chats. Those chats are held back, the
server is named, and the rest of the plane is restored:

    charter reopen: default.2 not reopened — the tmux server they are recorded on did not answer and is still there, so they may still be running on it (`tmux -L charter-plane-… attach` reaches it). They stay in the record: run `charter reopen` again once that server answers, or once it is stopped.

A launch no longer clears away the records of chats on a hung server either, since a
reopen needs those records to know which chats to hold back.

One case is still read as gone: a tmux server that keeps running after its socket file was
deleted, by a temp cleaner for example. Neither charter nor tmux itself can reach its chats
until `kill -USR1` recreates the socket.
