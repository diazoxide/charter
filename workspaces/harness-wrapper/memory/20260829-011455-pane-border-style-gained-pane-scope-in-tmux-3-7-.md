# pane-border-style gained pane scope in tmux 3.7 exactly: options-table.c

_2026-08-29 01:14 · persistent_

pane-border-style gained pane scope in tmux 3.7 exactly: options-table.c says OPTIONS_TABLE_WINDOW in 3.2/3.3a/3.4/3.5/3.6/3.6a and OPTIONS_TABLE_WINDOW|OPTIONS_TABLE_PANE from 3.7. Below 3.7 'set -p' is rc 0 and writes the WINDOW and 'set -p -u' removes the window's — a SILENT wrong-scope, so it needs a version gate on the WRITE (a probe would have to perform the damaging write). CI runs tmux 3.4, so any test forcing the pane-scoped path there must probe-and-skip, not assume.
