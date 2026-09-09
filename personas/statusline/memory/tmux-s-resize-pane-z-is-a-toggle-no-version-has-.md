# tmux's `resize-pane -Z` is a TOGGLE — no version has an unzoom flag, and

_2026-09-07 18:45 · persistent_

tmux's `resize-pane -Z` is a TOGGLE — no version has an unzoom flag, and cmd-resize-pane unzooms the WINDOW when the window is zoomed regardless of the -t pane. To unzoom idempotently, guard it: `if-shell -F -t <pane> '#{window_zoomed_flag}' 'resize-pane -Z -t <pane>'`. A bare toggle re-zooms a pane somebody already unzoomed.
