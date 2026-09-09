# tmux frame integration fixtures need TWO server-env things the launcher 

_2026-09-08 11:27 · persistent_

tmux frame integration fixtures need TWO server-env things the launcher normally provides, or they silently measure nothing: (1) set-option -w -t <pane> @charter_chat <fid> on every chat window — commands_frame._chat_seats reads that option, so without it _plane_live returns an empty live set, leave.stopping drops every chat, and a chat: close confirmation degrades to 'no chats are open on this plane — nothing to quit' which LOOKS exactly like the bug under test; (2) commands_frame._charter_py_env_argv — the F2 hotkey bind is run-shell '"$CHARTER_PY" -m charter …' expanded from the tmux SESSION environment, so an unset CHARTER_PY makes F2 a silent no-op. Same family as the TERM=dumb trap: assert a positive control before believing a negative result.
