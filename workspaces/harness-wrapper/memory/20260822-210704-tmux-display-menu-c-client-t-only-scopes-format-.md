# tmux display-menu -c <client>: -t only scopes format evaluation for an i

_2026-08-22 21:07 · persistent_

tmux display-menu -c <client>: -t only scopes format evaluation for an item's own command text, it does NOT choose which attached client sees the menu -- -c is what selects the client (display-menu's own docs: 'Display a menu on target-client. target-pane gives the target for any commands run from the menu'). Also: #{client_name} format expansion, embedded in a bind's own run-shell text (e.g. bind -n F2 run-shell 'mycmd "#{client_name}"'), resolves per-KEYPRESS to whichever client actually pressed the key -- verified with two real ptys attached to one session, each press resolving to its own presser regardless of attach order. This is the reliable way to carry 'who pressed' through a binding without querying list-clients afterward (which can only tell you who's attached, not who pressed).
