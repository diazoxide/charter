# Reproducing an 'outside a frame' charter bug from a Claude Code session 

_2026-08-31 02:41 · persistent_

Reproducing an 'outside a frame' charter bug from a Claude Code session INHERITS $CHARTER_SESSION_ID from the operator's live frame — the repro silently runs the in-frame path instead. env -u CHARTER_SESSION_ID (and -u TMUX/TMUX_PANE) before every such repro; tests/_envguard.unset_all() is the suite's version of the same guard.
