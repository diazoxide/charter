# A pty measurement that attaches a real tmux client fails silently on CI:

_2026-09-07 18:45 · persistent_

A pty measurement that attaches a real tmux client fails silently on CI: tmux refuses to start a client on a terminal it cannot look up (`open terminal failed`), and a runner's TERM is often dumb or absent — the read comes back b'' and the assertion measures nothing. Attach under a TERM ladder (operator's, then xterm-256color/screen/vt100), wait for `list-clients` to report one, and give the case a positive control on the state that already ships so an environment that cannot show the property skips instead of failing.
