# tmux refuses an over-long command in TWO sentences, not one (measured 20

_2026-09-10 23:44 · persistent_

tmux refuses an over-long command in TWO sentences, not one (measured 2026-09-10 on 3.7c and 3.2, through new-session, new-window and respawn-pane): command message (command name + each argument + NUL, global -L/-S/-f excluded) <= 16,364 bytes starts; 16,365..16,380 is rc 1 'failed to send command'; 16,381+ is rc 1 'command too long'. A classifier that recognises only one of them misses most real refusals. charter's tmuxctl.MESSAGE_LIMIT / refused_as_too_long (#957) carry this.
