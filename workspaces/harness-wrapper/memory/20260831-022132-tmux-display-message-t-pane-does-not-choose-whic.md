# tmux `display-message -t <pane>` does NOT choose which client sees the m

_2026-08-31 02:21 · persistent_

tmux `display-message -t <pane>` does NOT choose which client sees the message — `-t` is the target for FORMAT evaluation only; the client is `-c`, and with no `-c` tmux picks its own current client. Measured on tmux 3.7c AND 3.2, two sessions on one server with a terminal attached to each: a message aimed at a pane of session 'sa' was drawn on 'sb's terminal and not on 'sa's at all. Also measured: a tmux client suspends its PANE redraw for the entire duration of a message, exactly tracking -d (-d 4000 -> 4.0s frozen, -d 200 -> 0.20s), on both versions. Both facts found while fixing charter #729.
