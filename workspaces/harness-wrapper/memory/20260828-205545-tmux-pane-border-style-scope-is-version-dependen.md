# tmux pane-border-style scope is version-dependent and silently wrong at 

_2026-08-28 20:55 · persistent_

tmux pane-border-style scope is version-dependent and silently wrong at the floor: on 3.7c it IS a pane option and tmux draws each border cell from the pane ABOVE or LEFT of it (the other side is ignored); on tmux 3.2 (tmuxctl.FLOOR) it is NOT a pane option — 'set -p' returns 0 but writes the WINDOW's value and 'set -p -u' removes charter's own #514 window pin. So frame border styling must be window-scoped (-w). Measured 2026-08-28 for #627/PR 628.
