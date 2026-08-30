# tmux parses a -t target on '.' as window.pane, so a session or window NA

_2026-08-30 15:03 · persistent_

tmux parses a -t target on '.' as window.pane, so a session or window NAME containing a dot is never resolved as a name: 'new-window -t api.2' answers 'can't specify pane here' rc=1, 'set-environment -t api.2' lands on session 'api' with rc=0, and 'display-message -p -t api.2.1' resolves to session 'api'. Measured on tmux 3.7c. charter/instance.WORKSPACE_NAME_RE allows dots, so this is reachable — filed as charter#695.
