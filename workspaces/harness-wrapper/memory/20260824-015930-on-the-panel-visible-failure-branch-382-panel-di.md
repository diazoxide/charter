# On the panel-visible-failure branch (#382), _panel_died_hook_argv and cm

_2026-08-24 01:59 · persistent_

On the panel-visible-failure branch (#382), _panel_died_hook_argv and cmd_respawn hardcode ['tmux','-L',SOCKET,...] — never route through tmuxctl.server_argv. When rebasing onto #381 (operator's-own-tmux, socket can be a PATH needing -S), do NOT fold pane-died-hook arming into the shared _draw_panels helper: that would arm hooks on operator-tmux panels whose action targets the wrong tmux server. Keep it scoped to cmd_launch's private-server path only, using the panes dict _draw_panels already returns, until cmd_respawn is made socket-aware (state.frame_server(fid) + windows-vs-sessions liveness).
