---
version: unreleased
headline: A panel that dies inside the tmux you already have now comes back
---

Panels get their own `pane-died` hook and respawn with backoff — three attempts, then the
dead pane and its message are left where you can read them. Inside your own tmux no hook
was ever armed, so a panel that died there stayed dead for the life of the frame: no
message, no respawn, no backoff. Only the frame drawn as a window in your server was
affected; a plain terminal takes charter's private-server path, where this always worked.

The reason it was not simply extended is worth recording, because it is the shape of bug
that looks fixed in a diff. Both ends of the mechanism named charter's private server by
hand — `tmux -L charter` — while your server is reached by socket **path**, `tmux -S …`.
Arming the hook as written would have installed a `run-shell` pointing at a different tmux
server: one that may not exist, or that may be another frame's. So it was backed out
instead of guessed at.

Both ends now ask the one function that already knows `-L` from `-S`, and `charter
frame-respawn` resolves which server the frame is on from the frame's own record — asking
about **windows** on your server and **sessions** on charter's, because a frame is a window
in one and a session in the other. A server that does not answer at all is not treated as
an empty one: charter declines to respawn rather than aim a command at a tmux it could not
reach.

The hook also carries what it needs on its own command line now — the interpreter and the
frame id — rather than reading them back from tmux session variables. Those are set with
`set-environment`, which charter does not write on a server it is a guest on, so there was
no channel there at all (measured: a `run-shell` fired by a pane-scoped hook sees the
session environment and not the pane's own).

Which means charter's interpreter path is written into a string tmux later re-reads, and
charter refuses to arm a hook whose path holds a character that would not survive that
intact — saying so, rather than installing something that runs a command it did not mean. A
path with a space, a `;` or an `&` is fine and was measured arriving byte for byte; one
with `$( )` is not, and was measured executing. The first version of that check looked for
quote characters and would have shipped: a hook action passes through **three** parsers,
and the first is tmux's own `#{…}` format expansion — measured rewriting a literal
`/opt/py#{pane_id}/x` into `/opt/py%1/x` before any shell saw it, with `#{pane_title}`
expanding to text the program in that pane sets for itself. `#` is refused with the rest.
