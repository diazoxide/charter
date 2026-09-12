# capture-frame.sh private tmux (2026-09-11, branch readme-shows-the-frame

_2026-09-11 01:41 · persistent_

capture-frame.sh private tmux (2026-09-11, branch readme-shows-the-frame): the frame socket is the constant -L charter (commands_frame.SOCKET) with no override, but tmux and charter both honour $TMUX_TMPDIR (frame/tmuxctl.socket_path; _frame_env is dict(os.environ)), so exporting TMUX_TMPDIR to a mktemp dir under /tmp makes -L charter a private server a capture can kill-server whole. It cannot live under a Claude Code scratchpad: sockaddr_un is 104 bytes on macOS, that path measured 146 bytes and tmux refused with "File name too long". An agent shell inside the operator frame has TMUX pointing at the live charter server, so unset TMUX/TMUX_PANE and CHARTER_* before launching anything.
