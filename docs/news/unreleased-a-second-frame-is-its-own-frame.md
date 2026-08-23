---
version: unreleased
headline: A second frame on the same machine no longer answers to the first one's id
---

Switching workspace from inside `charter claude` updated the status line and left the
frame's panels showing the workspace they launched with. The panels were not stale for
want of repainting — they were reading the right pointer under the wrong name.

**Every frame on charter's own server is a session on ONE shared tmux server**, which
means exactly one launch per machine is the launch whose `new-session` actually starts
that server. charter carried its own variables to the harness by handing them to the tmux
client and letting the server inherit them — true of that first launch and of no other.
For every frame after it, tmux builds the new pane's environment from the SERVER's global
one, captured from whichever launcher started it, possibly days earlier.

Measured against tmux 3.7c with two frames on one socket: the second frame's harness
reported the FIRST frame's `$CHARTER_SESSION_ID`, while `show-environment -g` held the
same first id. Everything keyed on that variable then went to the wrong frame —
`charter workspace use` wrote the first frame's workspace pointer, and every hook bumped
the first frame's version counter, so the second frame's panels waited for a change that
was being recorded next door. The status line kept up because it falls through to a
per-terminal pointer keyed by the pane, which the harness's own `ws use` had written.

The frame's environment now rides on `new-session -e`, one `NAME=VALUE` per variable, the
same way the inside-your-own-tmux path already carried it on `respawn-pane -e`. The panels
were never the problem — they take their id from a session-scoped `set-environment` that
tmux does apply to panes split later — so this closes the one pane that call cannot reach:
the one `new-session` itself creates.

`new-session -e` arrived in tmux 3.2, and charter still launches below that (it warns and
degrades rather than refusing). Below 3.2 the flag is not ignored, it is a parse error
that would take the whole launch down, so the environment is withheld there and a second
frame behaves exactly as every frame did before this fix.

Nothing to adopt: upgrading is the whole of it. A frame already running keeps whatever id
it was started with — start it again to pick this up.
