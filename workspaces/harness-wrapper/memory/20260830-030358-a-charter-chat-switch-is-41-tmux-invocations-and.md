# A charter chat switch is 41 tmux invocations and ~360ms (3.7c) / ~395ms 

_2026-08-30 03:03 · persistent_

A charter chat switch is 41 tmux invocations and ~360ms (3.7c) / ~395ms (3.2), not the Phase 5 spec §7.7's ~16ms: that figure counted 9 commands (select-window + 4 kill-pane + 4 split-window) but _apply_arrangement/_relayout also issues the respawn disarm+arm per pane, the panel mark, pane surface/border options, _install_resize_hook and _reassert_sizes. So tmuxctl.chain over the kills and splits buys ~10%, not 3.3x — the rest is in the other 33 invocations.
