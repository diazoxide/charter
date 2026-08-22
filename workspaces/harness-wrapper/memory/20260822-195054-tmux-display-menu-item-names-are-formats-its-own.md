# tmux display-menu item NAMES are FORMATS (its own docs: 'The name and co

_2026-08-22 19:50 · persistent_

tmux display-menu item NAMES are FORMATS (its own docs: 'The name and command are formats'), not inert text -- an unescaped label containing #(shell cmd) runs the shell command the instant tmux DRAWS the menu (no selection needed), and #{var} substitutes a value. Fix: replace every '#' with '##' in the label (tmux's own escape for a literal #) before it reaches display-menu's argv. Also: -t <target-pane> on display-menu only scopes FORMAT evaluation for the item's own command text -- it does NOT choose which attached client sees the menu. -c <target-client> is what selects the client; verified two frames attached in two terminals: -t alone rendered frame B's menu on frame A's screen.
