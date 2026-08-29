# tmux draws each pane-border cell from exactly ONE pane's options: screen

_2026-08-29 19:39 · persistent_

tmux draws each pane-border cell from exactly ONE pane's options: screen_redraw_check_cell walks the window's pane list in order and takes the first whose border box contains the cell. charter's harness pane is created first, so it owns its own top/right/bottom border cells AND the parts of the identity and repos rules that run over its width. Leaving its pane-border-style unset does not give it dark edges — it gives one horizontal rule two colours. Measured on tmux 3.7c at the operator's real 203x69 geometry (charter #656).
