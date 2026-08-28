# capture-pane -p -e trims trailing spaces, so a tmux pane painted with wi

_2026-08-28 20:55 · persistent_

capture-pane -p -e trims trailing spaces, so a tmux pane painted with window-style and holding no text captures as a bare SGR with no cell to describe — use -N (works on 3.2 and 3.7c). And tmux does not re-state a background a row already inherits, so a line-scoped SGR parser reads a horizontal pane border drawn over a surface as the terminal's own default. Both bit the frame border test in PR 628.
