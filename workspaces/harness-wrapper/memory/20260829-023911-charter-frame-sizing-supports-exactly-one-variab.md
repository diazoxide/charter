# charter frame sizing supports exactly ONE variable-height pane (layout.V

_2026-08-29 02:39 · persistent_

charter frame sizing supports exactly ONE variable-height pane (layout.VARIABLE_ROW_SLOTS): slot_sizes answers every member with layout.repos_rows, and _reassert_sizes leaves that set unasserted because tmux resize-pane -y moves one boundary. A second Content()/Fill() row slot gets handed the repo table's height. Measured 2026-08-29 adding the 'changes' component: registered as Content() it got 6 rows for 6 repos on a plane with one change. New placed row components must be Fixed(n) until repos_rows/harness_rows/_reassert_sizes are redesigned together.
