# To ask where an ATTACHED tmux client is looking, use 'list-clients -F "#

_2026-09-11 07:48 · persistent_

To ask where an ATTACHED tmux client is looking, use 'list-clients -F "#{client_name} #{session_name} #{window_id} #{pane_id}"' (each client's own session, current window, active pane; it follows a select-window). Never 'display-message -p -c <client>': measured 2026-09-11 with two real pty clients, tmux 3.7c answers the server's CURRENT session for every client alike, and tmux 3.2 fails with rc 1 and prints nothing.
