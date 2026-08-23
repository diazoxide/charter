# In tests/test_frame_tmux_integration.py, addCleanup(kill-server) then ad

_2026-08-23 16:19 · persistent_

In tests/test_frame_tmux_integration.py, addCleanup(kill-server) then addCleanup(unlink-socket) runs LIFO (unlink first, kill-server second, backwards) — a kill-server issued after the socket path is already unlinked cannot reconnect to the still-live server. Harmless only when a session never arms remain-on-exit (killing the one pane process ends session+server on tmux's exit-empty default). Always use ONE combined kill-server-then-unlink cleanup (TmuxIntegration._teardown_socket is the model) for any tmux test class that arms remain-on-exit.
