---
version: unreleased
headline: A chat launched from a directory with `#` in its name starts in that directory
---

tmux reads the directory a chat starts in as a format, so a name with `#` in it could start
the chat somewhere else without a word. Measured on tmux 3.7c and 3.2, for a second chat in
a workspace and for a chat inside a tmux you already had: `x#{session_name}` started in
`xbase` when a directory by that name sat beside it and in `$HOME` when none did, `a#S` and
`a##b` went to `$HOME`, and on 3.7c a trailing `#` was dropped. A workspace's first chat is
handed to tmux with no directory and was not affected.

Charter now escapes the `#` tmux would read, as `##` — tmux's own spelling of a literal one
— and every name measured, `x#{session_name}`, `a#S`, `a##b`, `a#[b` and `#` among them,
now starts the chat in exactly that directory.

A directory named like a tmux shell job, `y#(…)`, went to `$HOME` too. Launched the way
charter launches — one tmux command, after which the client disconnects — its command did
not run in 30 launches per command; with the tmux client held connected, it ran in 5 out of
5. Escaped, it ran in none of 5 held launches, and the chat started in it.
