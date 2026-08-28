# tmux border-cell OWNERSHIP: screen_redraw_check_cell scans the pane list

_2026-08-29 01:14 · persistent_

tmux border-cell OWNERSHIP: screen_redraw_check_cell scans the pane list STARTING AT THE ACTIVE PANE and returns the first pane whose border ring contains the cell; 3.7 then resolves the style against THAT pane's options (3.2 always used w->options). So per-pane border colours are not 'the pane above/left' — measured on charter's four-panel frame, the harness (created first, and active) wins every ring it touches, giving it a dark top/right/bottom while panel|panel rules stay the panels' colour. Verified stable across 4 focus states.
