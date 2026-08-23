# tmux captures a client's environment into the SERVER only when new-sessi

_2026-08-24 03:24 · persistent_

tmux captures a client's environment into the SERVER only when new-session actually STARTS that server. Every later session on the same socket inherits the FIRST session's environment, not its own client's. Measured on 3.7c with two real charter frames: frame B's harness reported CHARTER_SESSION_ID=default-58069 while its own session was default-58696 — so 'ws use' wrote frame A's pointer and every hook bumped frame A's version (root cause of #411). Therefore any pane charter creates on a shared server MUST carry frame env explicitly: new-session -e AND respawn-pane -e. The -e flag landed in tmux 3.2, which is exactly charter's floor, so below 3.2 it is a PARSE ERROR rather than a missing feature and must be version-gated (tmuxctl.SESSION_ENV_FLOOR).
