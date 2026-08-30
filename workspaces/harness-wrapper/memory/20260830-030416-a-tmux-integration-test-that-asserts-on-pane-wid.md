# A tmux integration test that asserts on pane WIDTHS passes with dead pan

_2026-08-30 03:04 · persistent_

A tmux integration test that asserts on pane WIDTHS passes with dead panels: a charter panel spawned by _relayout dies with 'No module named charter' unless $PYTHONPATH carries the checkout (layout.panel_command builds the argv with -P, #390), and it must be set BEFORE the class's first tmux command because that command starts the server whose environment every later pane inherits. Second trap: point the panel at tests._isolation.make_plane (the case's OWN root), not child_plane_env — a fresh empty plane makes the panel scan an empty .charter/frame/ and draw a perfectly correct one-chat bar for a workspace with two. Only capture-pane on the painted text catches either.
