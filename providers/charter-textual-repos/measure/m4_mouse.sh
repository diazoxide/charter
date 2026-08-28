#!/usr/bin/env bash
# M4 — the mouse measurement. Builds the frame, then hands the socket to m4_mouse.py,
# which attaches a real tmux client on a pty it owns and reads every byte tmux writes to
# a terminal. See that file's docstring for what is being asked.
source "$(dirname "${BASH_SOURCE[0]}")/rig.sh"

seed
start_session
live=$(split_panel textual.live 14)
sleep 2.5
"$PY" "$HERE/m4_mouse.py" "$SOCK" "$live" "%0"
