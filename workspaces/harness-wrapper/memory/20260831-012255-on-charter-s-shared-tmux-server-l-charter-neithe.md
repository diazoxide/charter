# On charter's shared tmux server (-L charter) neither a session name nor 

_2026-08-31 01:22 · persistent_

On charter's shared tmux server (-L charter) neither a session name nor a chat id identifies a plane: session names are bare workspace names and new_chat_id counts from 1 on each plane's own disk, so two planes both mint 'default'/'default.1'. The only per-plane fact is the %<pane> id a plane's own launcher recorded in .charter/frame/<fid>/harness. Measured on 3.7c and 3.2: 'attach -t $N' does not move a session's current window; 'select-window' does.
