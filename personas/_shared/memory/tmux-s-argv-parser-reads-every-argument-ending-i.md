# tmux's argv parser reads EVERY argument ending in ';' as a command separ

_2026-09-11 02:35 · persistent_

tmux's argv parser reads EVERY argument ending in ';' as a command separator, not only the program's: measured 2026-09-11 on 3.7c and 3.2, a '-c <dir;>' or '-e NAME=value;' ends the command early (rc 1 'unknown command: -P' / '-e' / '--'), and a middle harness argument 'a;' makes the arguments after it run as a tmux command (set-option took effect, harness got ['a'], rc 0). Escape with a backslash before the last ';' (charter tmuxctl.verbatim, #957/#959). Separately: 'display-message -p -t %N' on a pane that no longer exists answers rc 0 with an empty line, so it cannot test whether a pane exists; use 'list-panes -a -F #{pane_id}' membership.
