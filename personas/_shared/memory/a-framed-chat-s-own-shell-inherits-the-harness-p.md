# A framed chat's own shell inherits the harness pane's tmux identity, so

_2026-09-11 14:05 · persistent_

A framed chat's own shell inherits the harness pane's tmux identity, so TMUX_PANE and CHARTER_SESSION_ID never prove a process IS that pane. Measured 2026-09-11 in chat harness-profiles.1: the model's Bash tool shell (pid 4707, ppid 53118) saw TMUX_PANE=%3195, and tmux display-message reported that pane's pane_pid as 53118 (the harness). Any charter command a model runs from its tool therefore passes a pane-id check for its own chat. To prove a process is a pane's first process, compare os.getpid() to #{pane_pid} read from tmux itself (list-panes -a on charter's server by socket name), before any exec. Guard rail only: a process that starts its own pane named like a chat still passes.
