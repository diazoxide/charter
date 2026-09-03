# Measured on tmux 3.7c: run-shell shows a child's STDOUT and discards its

_2026-08-31 02:41 · persistent_

Measured on tmux 3.7c: run-shell shows a child's STDOUT and discards its STDERR entirely, and prints "'<whole command>' returned N" into the pane for any non-zero exit. So a charter frame-* command can safely write a refusal to stderr on every path, but must not exit non-zero on any path tmux drives. Isolate a frame test from the operator's live 'charter' socket with a SHORT $TMUX_TMPDIR (commands_frame.SOCKET is a constant; the unix socket path limit is ~104 bytes, so a scratchpad path is too long).
