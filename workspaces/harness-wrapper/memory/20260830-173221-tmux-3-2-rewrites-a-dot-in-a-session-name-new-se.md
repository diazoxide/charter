# tmux 3.2 REWRITES a dot in a session name: 'new-session -s api.2' create

_2026-08-30 17:32 · persistent_

tmux 3.2 REWRITES a dot in a session name: 'new-session -s api.2' creates a session actually named 'api_2', while 3.7c keeps the dot and then splits every -t on it. So a trailing ':' disambiguates the target on 3.7c and finds nothing on 3.2 — a target-side fix for a dotted session name works on one of the two versions charter supports. The fix belongs in what charter MINTS (state.workspace_prefix), not in how it spells a target. Filed and fixed as charter#695.
